use std::fmt;

use super::{MerkleDepth, MerkleIndex};

#[derive(Debug)]
pub enum HashError {
    InputSizeError(usize),
    GeneralError(String),
}

impl fmt::Display for HashError {
    fn fmt(self: &Self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            HashError::InputSizeError(inp) => format!("invalid input size: {}", inp),
            HashError::GeneralError(inp) => format!("hash error: {}", inp),
        };
        write!(f, "{}", msg)
    }
}

#[derive(Debug)]
pub enum MerkleTreeError {
    TreeDepth(MerkleDepth),
    LeafIndex(MerkleIndex),
    HashError(HashError),
}

impl fmt::Display for MerkleTreeError {
    fn fmt(self: &Self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            MerkleTreeError::TreeDepth(h) => format!("tree depth is invalid: {}", h),
            MerkleTreeError::LeafIndex(i) => format!("leaf index is invalid: {}", i),
            MerkleTreeError::HashError(e) => format!("hash error: {}", e),
        };
        write!(f, "{}", msg)
    }
}

impl From<HashError> for MerkleTreeError {
    fn from(error: HashError) -> Self {
        MerkleTreeError::HashError(error)
    }
}

#[derive(Debug)]
pub enum MerkleTreeAVDError {
    OpenAddressingOverflow([u8; 32]),
    UpdateBatchSize(u64),
    ProofFormat,
    MerkleTreeError(MerkleTreeError),
    HashError(HashError),
}

impl fmt::Display for MerkleTreeAVDError {
    fn fmt(self: &Self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            MerkleTreeAVDError::OpenAddressingOverflow(k) => {
                format!("all open addressing probes populated for key: {:?}", k)
            }
            MerkleTreeAVDError::UpdateBatchSize(s) => format!("surpassed max batch size: {}", s),
            MerkleTreeAVDError::ProofFormat => "invalid proof format".to_string(),
            MerkleTreeAVDError::MerkleTreeError(e) => format!("merkle tree error: {}", e),
            MerkleTreeAVDError::HashError(e) => format!("hash error: {}", e),
        };
        write!(f, "{}", msg)
    }
}

impl From<MerkleTreeError> for MerkleTreeAVDError {
    fn from(error: MerkleTreeError) -> Self {
        MerkleTreeAVDError::MerkleTreeError(error)
    }
}

impl From<HashError> for MerkleTreeAVDError {
    fn from(error: HashError) -> Self {
        MerkleTreeAVDError::HashError(error)
    }
}