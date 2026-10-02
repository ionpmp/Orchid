//! Shared concept folding for the stub and the ONNX hash encoder.

/// Map a lowercase token onto its concept, when the table has one.
pub(crate) fn fold_concept(token: &str) -> &str {
    match token {
        "canine" | "puppy" | "hound" | "pup" => "dog",
        "feline" | "kitten" | "kitty" => "cat",
        "automobile" | "vehicle" | "auto" => "car",
        _ => token,
    }
}

pub(crate) fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

pub(crate) fn l2_normalize(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}
