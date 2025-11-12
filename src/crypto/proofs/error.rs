#[derive(Debug)]
pub enum VerificationError {
    // proof rejected
    Proof(String),
}

impl std::fmt::Display for VerificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            VerificationError::Proof(msg) => write!(f, "Proof rejected: {:?}", msg),
        }
    }
}

impl std::error::Error for VerificationError {}
