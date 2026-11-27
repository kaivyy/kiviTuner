use md5::Md5;
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BinaryHashes {
    pub md5: String,
    pub sha1: String,
    pub sha256: String,
}

pub fn compute_hashes(data: &[u8]) -> BinaryHashes {
    let md5 = format!("{:x}", Md5::digest(data));
    let sha1 = format!("{:x}", Sha1::digest(data));
    let sha256 = format!("{:x}", Sha256::digest(data));
    BinaryHashes { md5, sha1, sha256 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_hashes() {
        let data = b"hello world";
        let hashes = compute_hashes(data);
        assert_eq!(hashes.md5, "5eb63bbbe01eeed093cb22bb8f5acdc3");
        assert_eq!(hashes.sha1, "2aae6c35c94fcfb415dbe95f408b9ce91ee846ed");
        assert_eq!(
            hashes.sha256,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }
}
