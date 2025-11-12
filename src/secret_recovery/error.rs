use ark_groth16::Groth16;
use ed25519_dalek::SignatureError;

use crate::crypto::nivss::error::NIVSSError;
use crate::crypto::proofs::error::VerificationError as ProofVerificationError;
use crate::crypto::sortition::SortitionError;
use crate::crypto::ibe::error::IBEError;
use crate::secret_recovery::common::E;

#[derive(Debug)]
pub enum ECPSSClientError {
    InvalidCommittee(InvalidCommittee),
    InvalidState(InvalidState),
    InvalidRecoveryRequest(InvalidRecoveryRequest),
}

#[derive(Debug)]
pub enum InvalidCommittee {
    DuplicateEntries,
    MerkleProof(Option<crate::crypto::avd::error::MerkleTreeAVDError>),
    SmallerThanThreshold,
    SortitionProof(SortitionError),
    Proofs(ProofVerificationError),
    Signature,
}

impl std::fmt::Display for InvalidCommittee {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            InvalidCommittee::DuplicateEntries => write!(f, "Duplicate entries"),
            InvalidCommittee::MerkleProof(error) => write!(f, "Merkle proof error: {:?}", error),
            InvalidCommittee::SmallerThanThreshold => {
                write!(f, "Committee size smaller than threshold")
            }
            InvalidCommittee::SortitionProof(error) => {
                write!(f, "Sortition proof error: {:?}", error)
            }
            InvalidCommittee::Proofs(error) => write!(f, "Proofs error: {:?}", error),
            InvalidCommittee::Signature => write!(f, "Invalid signature"),
        }
    }
}

#[derive(Debug)]
pub enum InvalidState {
    DuplicateEntries,
    DuplicateSeats,
    ShareCommitmentsMismatch,
    CoeffCommitmentsMismatch,
    SmallerThanThreshold,
    SortitionProof(SortitionError),
    Signature(SignatureError),
    NIVSS(NIVSSError),
    DLEQProof(ProofVerificationError),
    DecryptionMismatchWithCommitment(usize),
}

impl std::fmt::Display for InvalidState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            InvalidState::DuplicateEntries => write!(f, "Duplicate entries"),
            InvalidState::DuplicateSeats => write!(f, "Duplicate seats"),
            InvalidState::ShareCommitmentsMismatch => write!(f, "Share commitments mismatch"),
            InvalidState::CoeffCommitmentsMismatch => write!(f, "Coeff commitments mismatch"),
            InvalidState::SmallerThanThreshold => write!(f, "State size smaller than threshold"),
            InvalidState::SortitionProof(error) => write!(f, "Sortition proof error: {:?}", error),
            InvalidState::Signature(error) => write!(f, "Invalid signature: {:?}", error),
            InvalidState::NIVSS(error) => write!(f, "NIVSS error: {:?}", error),
            InvalidState::DLEQProof(error) => write!(f, "DLEQ proof error: {:?}", error),
            InvalidState::DecryptionMismatchWithCommitment(seat_idx) => write!(
                f,
                "Decryption mismatch with commitment on seat_idx: {}",
                seat_idx
            ),
        }
    }
}

#[derive(Debug)]
pub enum InvalidRecoveryRequest {
    IBEError(IBEError),
    GrothError,
    TooManyAttempts,
    MerkleProof,
    RootMismatchWithNoRequests,
}

impl std::fmt::Display for ECPSSClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ECPSSClientError::InvalidCommittee(error) => write!(f, "{}", error),
            ECPSSClientError::InvalidState(error) => write!(f, "{}", error),
            ECPSSClientError::InvalidRecoveryRequest(error) => write!(f, "{:?}", error),
        }
    }
}

impl std::error::Error for ECPSSClientError {}

impl From<InvalidCommittee> for ECPSSClientError {
    fn from(error: InvalidCommittee) -> Self {
        ECPSSClientError::InvalidCommittee(error)
    }
}

impl From<InvalidState> for ECPSSClientError {
    fn from(error: InvalidState) -> Self {
        ECPSSClientError::InvalidState(error)
    }
}

#[derive(Debug)]
pub enum SecretRecoveryClientError {
    IBEError(IBEError),
    AESGCMError(aes_gcm::Error),
}

impl From<IBEError> for SecretRecoveryClientError {
    fn from(error: IBEError) -> Self {
        SecretRecoveryClientError::IBEError(error)
    }
}

impl From<aes_gcm::Error> for SecretRecoveryClientError {
    fn from(error: aes_gcm::Error) -> Self {
        SecretRecoveryClientError::AESGCMError(error)
    }
}