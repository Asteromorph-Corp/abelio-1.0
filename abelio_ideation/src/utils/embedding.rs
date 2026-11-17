use futures::future::try_join_all;
use tonic::transport::Channel;

use crate::tei::EmbedRequest;
use crate::tei::embed_client::EmbedClient;

pub async fn initialize_embed_client() -> anyhow::Result<EmbedClient<Channel>> {
    let tei_url = std::env::var("TEI_URL").expect("TEI_URL must be set");
    let client = EmbedClient::connect(tei_url.clone()).await?;

    Ok(client)
}

pub async fn embed_string(
    input: &str,
    mut client: EmbedClient<Channel>,
) -> anyhow::Result<Vec<f32>> {
    let request = EmbedRequest {
        inputs: input.to_string(),
        truncate: false,
        normalize: true,
        truncation_direction: 0,
        prompt_name: None,
        dimensions: None,
    };

    let resp = client.embed(request).await?.into_inner().embeddings;
    Ok(resp)
}

pub async fn embed_strings(
    inputs: &[String],
    client: EmbedClient<Channel>,
) -> anyhow::Result<Vec<Vec<f32>>> {
    let futs = inputs.iter().map(|s| {
        let client = client.clone();
        async move { embed_string(s, client).await }
    });

    let results: Vec<Vec<f32>> = try_join_all(futs).await?;

    Ok(results)
}
