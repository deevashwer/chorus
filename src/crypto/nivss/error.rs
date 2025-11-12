use crate::crypto::proofs::error::VerificationError as ProofVerificationError;
use ed25519_dalek::SignatureError;

#[derive(Debug)]
pub enum NIVSSError {
    ProofVerificationError(ProofVerificationError),
    SignatureVerificationError(SignatureError),
    ShareCommitmentsMismatch,
}

impl std::fmt::Display for NIVSSError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            NIVSSError::ProofVerificationError(msg) => write!(f, "Proof verification error: {:?}", msg),
            NIVSSError::SignatureVerificationError(msg) => write!(f, "Signature verification error: {:?}", msg),
            NIVSSError::ShareCommitmentsMismatch => write!(f, "Share commitments mismatch"),
        }
    }
}

impl From<ProofVerificationError> for NIVSSError {
    fn from(error: ProofVerificationError) -> Self {
        NIVSSError::ProofVerificationError(error)
    }
}

impl From<SignatureError> for NIVSSError {
    fn from(error: SignatureError) -> Self {
        NIVSSError::SignatureVerificationError(error)
    }
}