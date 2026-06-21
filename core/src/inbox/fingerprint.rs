use sha2::{Digest, Sha256};

/// Compute a stable content fingerprint for a collection from member file hashes.
pub fn compute_collection_fingerprint(file_hashes: &[String]) -> String {
    let mut hashes: Vec<&str> = file_hashes.iter().map(String::as_str).collect();
    hashes.sort_unstable();

    let mut payload = String::from("v1:files=");
    payload.push_str(&hashes.join(","));
    payload.push_str(";collections=");

    format!("{:x}", Sha256::digest(payload.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_order_independent() {
        let a = compute_collection_fingerprint(&[
            "aaa".into(),
            "bbb".into(),
        ]);
        let b = compute_collection_fingerprint(&[
            "bbb".into(),
            "aaa".into(),
        ]);
        assert_eq!(a, b);
    }

    #[test]
    fn fingerprint_differs_for_different_content() {
        let a = compute_collection_fingerprint(&["aaa".into()]);
        let b = compute_collection_fingerprint(&["bbb".into()]);
        assert_ne!(a, b);
    }
}
