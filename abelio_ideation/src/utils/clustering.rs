use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::{StreamExt, stream};
use rand::seq::SliceRandom;
use rayon::iter::{IndexedParallelIterator, IntoParallelRefMutIterator, ParallelIterator};
use tonic::transport::Channel;

use crate::tei::embed_client::EmbedClient;
use crate::utils::{RerankerCtx, embed_strings};

/// Performs K-Means clustering on flattened data.
///
/// # Arguments
/// * `data` - Flattened 1D slice [x1, y1, z1, x2, y2, z2, ...]
/// * `n` - Number of data points; `data.len()` must equal `n * dim`, which the unsafe blocks below
///   rely on and nothing here checks
/// * `dim` - Dimensionality of each point
/// * `k` - Number of clusters
/// * `max_iterations` - Maximum refinement steps
pub fn kmeans_clustering_optimized(
    data: &[f32],
    n: usize,
    dim: usize,
    k: usize,
    max_iterations: usize,
) -> anyhow::Result<Vec<usize>> {
    if data.is_empty() || k == 0 {
        return Ok(Vec::new());
    }

    // Initialize centroids by randomly sampling k points from the data.
    // Centroids are stored in a flat 1D vector for cache locality.
    let mut centroids = vec![0.0f32; k * dim];

    // Efficiently sample unique indices
    let mut indices: Vec<usize> = (0..n).collect();
    indices.shuffle(&mut rand::rng());

    for (i, &idx) in indices.iter().take(k).enumerate() {
        let start = idx * dim;
        // SAFETY: `idx < n` and `data.len() == n * dim`, so the source range `[idx * dim, (idx + 1)
        // * dim)` is in bounds. `i < k` because of `take(k)`, so the destination range lies within
        // `centroids`, which holds `k * dim` floats. The two buffers are separate allocations, so
        // the ranges cannot overlap.
        unsafe {
            std::ptr::copy_nonoverlapping(
                data.as_ptr().add(start),
                centroids.as_mut_ptr().add(i * dim),
                dim,
            );
        }
    }

    // Cluster assignments for each data point
    let mut assignments = vec![0usize; n];

    // Pre-allocate buffers for the M-Step to avoid allocations inside the loop
    let mut counts = vec![0usize; k];
    let mut new_centroids_sum = vec![0.0f32; k * dim];

    for _ in 0..max_iterations {
        // Atomic flag to track if any point changed its cluster
        let changed = AtomicBool::new(false);

        // E-step: assign each point to its nearest centroid, in parallel across all CPU cores.
        assignments
            .par_iter_mut()
            .enumerate()
            .for_each(|(i, assignment)| {
                let point_offset = i * dim;
                let mut min_dist_sq = f32::MAX;
                let mut best_cluster = 0;

                for c in 0..k {
                    let centroid_offset = c * dim;
                    let mut dist_sq = 0.0;

                    // SAFETY: `i < n` because `assignments` has `n` elements, so
                    // `data[point_offset..point_offset + dim]` is in bounds given `data.len() == n
                    // * dim`. `c < k`, so `centroids[centroid_offset..centroid_offset + dim]` is in
                    // bounds of the `k * dim` centroid buffer. Both buffers are only read while the
                    // parallel loop runs.
                    unsafe {
                        let p_ptr = data.as_ptr().add(point_offset);
                        let c_ptr = centroids.as_ptr().add(centroid_offset);

                        for d in 0..dim {
                            let diff = *p_ptr.add(d) - *c_ptr.add(d);
                            dist_sq += diff * diff;
                        }
                    }

                    if dist_sq < min_dist_sq {
                        min_dist_sq = dist_sq;
                        best_cluster = c;
                    }
                }

                if *assignment != best_cluster {
                    *assignment = best_cluster;
                    changed.store(true, Ordering::Relaxed);
                }
            });

        // Early exit if convergence is reached
        if !changed.load(Ordering::Relaxed) {
            break;
        }

        // M-step: recompute each centroid as the mean of its assigned points.
        counts.fill(0);
        new_centroids_sum.fill(0.0);

        // Accumulate coordinates for each cluster
        for (i, &cluster_idx) in assignments.iter().enumerate() {
            // SAFETY: every assignment is `0` or a `best_cluster` drawn from `0..k`, and `k >= 1`
            // here, so `cluster_idx < k`. That keeps `counts` (length `k`) and `new_centroids_sum`
            // (length `k * dim`) in bounds. `i < n`, so the read from `data` is in bounds given
            // `data.len() == n * dim`.
            unsafe {
                *counts.get_unchecked_mut(cluster_idx) += 1;

                let p_ptr = data.as_ptr().add(i * dim);
                let sum_ptr = new_centroids_sum.as_mut_ptr().add(cluster_idx * dim);

                for d in 0..dim {
                    *sum_ptr.add(d) += *p_ptr.add(d);
                }
            }
        }

        // Calculate new means (averages) for the centroids
        #[allow(clippy::needless_range_loop)]
        for c in 0..k {
            let count = counts[c];
            if count > 0 {
                let factor = 1.0 / count as f32;
                // SAFETY: `c < k`, and both `centroids` and `new_centroids_sum` hold `k * dim`
                // floats, so the `dim`-float ranges at `c * dim` are in bounds. The two buffers are
                // separate allocations.
                unsafe {
                    let c_ptr = centroids.as_mut_ptr().add(c * dim);
                    let sum_ptr = new_centroids_sum.as_ptr().add(c * dim);

                    for d in 0..dim {
                        *c_ptr.add(d) = *sum_ptr.add(d) * factor;
                    }
                }
            }
            // An empty cluster keeps its previous position.
        }
    }

    Ok(assignments)
}

pub async fn cluster_strings(
    inputs: &[String],
    k: usize,
    client: EmbedClient<Channel>,
) -> anyhow::Result<Vec<Vec<String>>> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    if k == 0 {
        anyhow::bail!("k (cluster count) must be >= 1");
    }

    let vecs = embed_strings(inputs, client).await?;

    let actual_k = k.min(vecs.len());
    let assignments = tokio::task::spawn_blocking(move || {
        kmeans_clustering_optimized(&vecs.concat(), vecs.len(), vecs[0].len(), actual_k, 100)
    })
    .await??;

    let mut clusters: Vec<Vec<String>> = vec![Vec::new(); actual_k];
    for (idx, &cluster_id) in assignments.iter().enumerate() {
        clusters[cluster_id].push(inputs[idx].clone());
    }

    clusters.retain(|c| !c.is_empty());

    Ok(clusters)
}

pub async fn cluster_then_pick_k(
    inputs: &[String],
    query: impl Into<Cow<'_, str>>,
    num_clusters: usize,
    per_cluster: usize,
    rerank_concurrency: usize,
    client: EmbedClient<Channel>,
) -> anyhow::Result<Vec<String>> {
    let clustered = cluster_strings(inputs, num_clusters, client).await?;

    let reranker_ctx = RerankerCtx::new().await?;
    let query = query.into();

    let distilled_elements = stream::iter(clustered)
        .map(|cluster| {
            let reranker_ctx = reranker_ctx.clone();
            let query = query.clone();
            async move {
                reranker_ctx
                    .top_n(&query, &cluster, per_cluster)
                    .await
                    .inspect_err(|err| tracing::warn!(error = %err, "element distillation failed"))
                    .ok()
            }
        })
        .buffered(rerank_concurrency)
        .flat_map(|opt| stream::iter(opt.into_iter().flatten()))
        .collect::<Vec<String>>()
        .await;

    Ok(distilled_elements)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::initialize_embed_client;

    #[tokio::test]
    #[ignore = "live: needs a TEI server at TEI_URL"]
    async fn test_cluster_strings_with_embeddings() {
        dotenvy::dotenv().ok();
        let _ = tracing_subscriber::fmt().with_test_writer().try_init();

        let inputs = vec![
            "apple",
            "banana",
            "grape",
            "car",
            "bus",
            "train",
            "orange",
            "strawberry",
            "bicycle",
            "mango",
            "truck",
            "watermelon",
            "peach",
            "motorcycle",
            "kiwi",
        ];

        let inputs = inputs
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<String>>();

        let client = initialize_embed_client().await.unwrap();
        let clusters = cluster_strings(&inputs, 12, client).await.unwrap();

        println!("Clusters: {:?}", clusters);
    }
}
