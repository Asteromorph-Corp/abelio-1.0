use rand::seq::SliceRandom;

pub fn sample_by_ratio<T>(mut vec: Vec<T>, ratio: f64) -> Vec<T> {
    debug_assert!(
        (0.0..=1.0).contains(&ratio),
        "Ratio must be between 0.0 and 1.0"
    );

    let len = vec.len();
    let take = ((len as f64) * ratio).ceil() as usize;

    if take == 0 {
        vec.clear();
        return vec;
    }
    if take >= len {
        return vec;
    }

    let mut rng = rand::rng();
    let half_len = len / 2;

    if take <= half_len {
        vec.partial_shuffle(&mut rng, take);
        vec.truncate(take);
    } else {
        let drop_count = len - take;
        vec.partial_shuffle(&mut rng, drop_count);
        vec.drain(0..drop_count);
    }

    vec
}
