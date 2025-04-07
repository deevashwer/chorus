use aes_gcm::{AeadCore, Aes256Gcm, AesGcm, Nonce};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_std::{cfg_into_iter, iterable::Iterable, One};
use blake2b_rs::Blake2bBuilder;
use class_group::{CL_HSM_Ciphertext, CL_HSM_PublicKey, CL_HSM};
use ed25519_dalek::ed25519::signature::Keypair;
use openssl::bn::BigNum;
use rand::{rngs::StdRng, SeedableRng};
use std::{
    fmt::Debug,
    ops::{Div, Mul},
    marker::PhantomData
};
use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use serde::{Serialize, Deserialize, de};
use sha2::{digest::typenum::Unsigned, Sha256};

use crate::{cfg_into_iter_client, crypto::avd::{sparse_merkle_tree::SparseMerkleTree, LookupProof}};
use crate::crypto::avd::{MerkleTreeAVD, MerkleTreeAVDParameters, UpdateProof, sparse_merkle_tree::{MerkleTreeParameters, MerkleDepth, MerkleTreePath, MerkleIndex, CRHFromDigest}};
use crate::crypto::ibe::constraints::RecoveryRequestNIZK;
use crate::crypto::ibe::{Blind, BlindExtractionRequest, BlindExtractionResponse, CipherText as IBECiphertext};
use crate::crypto::nivss::sa_nivss::{Dealing, DealingLite};
use crate::crypto::proofs::{CLKoEProof, ChaumPedersen, DLEQInstance, DLEQProof, FeldmanCommitment};
use crate::crypto::shamir::ShamirSecretSharing;
use crate::crypto::sortition::{SortitionOutput, VRFPublicKey};
pub use ark_bls12_377::Config as P;
pub use ark_bls12_377::Bls12_377 as E;
pub use ark_bw6_761::BW6_761 as E2;
pub use ark_bls12_377::Fr;
use ark_ec::{pairing::Pairing, VariableBaseMSM, short_weierstrass::Affine, bls12::Bls12Config};
pub type G1Affine = <E as Pairing>::G1Affine;
pub type ScalarField = <E as Pairing>::ScalarField;
use ark_ff::ToConstraintField;
use ark_crypto_primitives::snark::SNARK;
use ark_groth16::{Groth16, Proof as Groth16Proof, ProvingKey as GrothProvingKey, VerifyingKey as GrothVerifyingKey};

#[cfg(feature = "parallel")]
use rayon::prelude::*;

#[derive(Debug, Clone)]
pub struct SystemParams<'a> {
    pub num_clients: usize,
    pub committee_size: usize,
    pub threshold: usize,
    pub id_len: usize,
    pub pwd_len: usize,
    pub cl_params: CL_HSM,
    pub g1_gen: G1Affine,
    pub g1_genprime: G1Affine,
    pub guess_limit: usize,
    pub groth_vk: GrothVerifyingKey<E2>,
    pub groth_pk: &'a GrothProvingKey<E2>,
}

pub fn seed_hash(data: &Vec<u8>, epoch: usize) -> [u8; 32] {
    let mut buf = [0u8; 32];
    let mut hasher = Blake2bBuilder::new(32).personal(b"SEED").build();
    hasher.update(&epoch.to_le_bytes());
    hasher.update(data);
    hasher.finalize(&mut buf);
    buf.into()
}

pub type H = CRHFromDigest<Sha256>;

#[derive(Clone)]
pub struct MerkleTreeGuessAttemptsParameters;

impl MerkleTreeParameters for MerkleTreeGuessAttemptsParameters {
    const DEPTH: MerkleDepth = 32;
    type H = H;
}

#[derive(Clone)]
pub struct MerkleTreeAVDGuessAttemptsParameters;

impl MerkleTreeAVDParameters for MerkleTreeAVDGuessAttemptsParameters {
    const MAX_UPDATE_BATCH_SIZE: u64 = 1000;
    const MAX_OPEN_ADDRESSING_PROBES: u8 = 16;
    type MerkleTreeParameters = MerkleTreeGuessAttemptsParameters;
}

pub type GuessAttemptsMerkleTreeAVD = MerkleTreeAVD<MerkleTreeAVDGuessAttemptsParameters>;

#[derive(Clone)]
pub struct MerkleTreeClientListParameters;

impl MerkleTreeParameters for MerkleTreeClientListParameters {
    const DEPTH: MerkleDepth = 32;
    type H = H;
}

#[derive(Clone)]
pub struct MerkleTreeAVDClientListParameters;

impl MerkleTreeAVDParameters for MerkleTreeAVDClientListParameters {
    const MAX_UPDATE_BATCH_SIZE: u64 = 1000;
    const MAX_OPEN_ADDRESSING_PROBES: u8 = 16;
    type MerkleTreeParameters = MerkleTreeClientListParameters;
}

pub type ClientListMerkleTreeAVD = MerkleTreeAVD<MerkleTreeAVDClientListParameters>;

// pub type ClientListMerkleTree = SparseMerkleTree<MerkleTreeClientListParameters>;
// pub type ClientListMerkleProof = MerkleTreePath<MerkleTreeClientListParameters>;

// TODO: include anti-sybil proof; define the method for verifying all information about it with this struct
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ECPSSClientData {
    pub id: usize,
    pub vrf_pubkey: VRFPublicKey,
    pub sig_pubkey: VerifyingKey,
    pub pke_pubkey: CL_HSM_PublicKey,
    pub pubkey_proof: CLKoEProof,
}

impl ECPSSClientData {
    pub fn to_bytes_for_hashing(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&bincode::serialize(&self.id).unwrap());
        buf.extend_from_slice(&self.vrf_pubkey.pk);
        buf.extend_from_slice(&bincode::serialize(&self.sig_pubkey).unwrap());
        buf.extend_from_slice(&bincode::serialize(&self.pke_pubkey).unwrap());
        buf
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CommitteeData {
    pub epoch: usize,
    pub root: Vec<u8>,
    pub members: Vec<(ECPSSClientData, SortitionProof)>,
    pub merkle_proof: Vec<LookupProof<MerkleTreeAVDClientListParameters>>,
}

impl Debug for CommitteeData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommitteeData")
            .field("epoch", &self.epoch)
            .field("committee size", &self.get_comm_size())
            .field(
                "client indices",
                &self
                    .members
                    .iter()
                    .map(|(c, _)| c.id)
                    .collect::<Vec<usize>>(),
            )
            .finish()
    }
}

impl CommitteeData {
    pub fn get_comm_size(&self) -> usize {
        self.members.len()
    }

    pub fn bytesize_without_merkle_proof(&self) -> usize {
        (bincode::serialized_size(&self).unwrap() - bincode::serialized_size(&self.merkle_proof).unwrap()) as usize
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct SortitionProof {
    pub epoch: usize,
    pub client_id: usize,
    pub srt_output: SortitionOutput,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Handover {
    pub epoch: usize,
    pub client_id: usize,
    pub seat_idx: usize,
    pub dealing: Dealing,
    pub dleq_proof: Option<DLEQProof>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HandoverLite {
    pub epoch: usize,
    pub client_id: usize,
    pub seat_idx: usize,
    pub dealing: DealingLite,
    pub dleq_proof: Option<DLEQProof>,
}

pub use crate::crypto::proofs::FeldmanCommitment as CommitteeShareCommitment;

use super::client::pad_to_length;
use super::error::{ECPSSClientError, InvalidRecoveryRequest, InvalidState};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PublicState {
    pub merkle_root: Vec<u8>,
}

impl PublicState {
    pub fn to_bytes_le(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend(self.merkle_root.clone());
        bytes
    }
    
    // 32 bytes
    pub fn from_bytes_le(bytes: &[u8]) -> Self {
        Self { merkle_root: bytes[..32].to_vec() }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.to_bytes_le()
    }

    pub fn bytesize(&self) -> usize {
        self.to_bytes().len()
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self::from_bytes_le(bytes)
    }

    pub fn to_bytes_for_mt(&self) -> [u8; 32] {
        let root: [u8; 32] = self.merkle_root.clone().try_into().unwrap();
        root
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CoefficientCommitments {
    pub coeff_cmts: Vec<FeldmanCommitment>,
}

// CommitteeState sent to client
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommitteeStateClient {
    pub epoch: usize,
    pub handovers: Vec<HandoverLite>,
    pub public_state: PublicState,
    // coeff_cmts from these handovers
    pub coeff_cmts: CoefficientCommitments,
}

// committee state maintained by the server
#[derive(Clone, Serialize, Deserialize)]
pub struct CommitteeStateServer {
    pub epoch: usize,
    pub handovers: Vec<Handover>,
    pub public_state: PublicState,
    // coeff_cmts from these handovers
    pub coeff_cmts: CoefficientCommitments,
}

// compute share commitments for indices
pub fn compute_share_commitments(
    t: usize,
    indices: &Vec<usize>,
    coeff_cmts: &Vec<G1Affine>,
) -> Vec<CommitteeShareCommitment> {
    let mut powers_of_idx = Vec::new();
    for (i, idx) in indices.iter().enumerate() {
        powers_of_idx.push(vec![<E as Pairing>::ScalarField::one(); t]);
        let val = <E as Pairing>::ScalarField::from(*idx as u64);
        for j in 1..t {
            powers_of_idx[i][j] = powers_of_idx[i][j - 1] * &val;
        }
    }
    let share_cmts = cfg_into_iter_client!(indices).enumerate().map(|(i, idx)| {
        let share_cmt = CommitteeShareCommitment {
            cmt: <E as Pairing>::G1::msm(&coeff_cmts, &powers_of_idx[i])
                .unwrap()
                .into(),
        };
        share_cmt
    }).collect::<Vec<CommitteeShareCommitment>>();
    /*
    let mut share_cmts = Vec::new();
    for (i, idx) in indices.iter().enumerate() {
        let share_cmt = CommitteeShareCommitment {
            cmt: <E as Pairing>::G1::msm(&coeff_cmts, &powers_of_idx[i])
                .unwrap()
                .into(),
        };
        share_cmts.push(share_cmt);
    }
    */
    share_cmts
}

pub trait HandoverType {
    fn get_coeff_cmt(&self) -> &Vec<FeldmanCommitment>;
    fn get_seat_idx(&self) -> usize;
}

impl HandoverType for Handover {
    fn get_coeff_cmt(&self) -> &Vec<FeldmanCommitment> {
        &self.dealing.coeff_cmt
    }
    fn get_seat_idx(&self) -> usize {
        self.seat_idx
    }
}

impl HandoverType for HandoverLite {
    fn get_coeff_cmt(&self) -> &Vec<FeldmanCommitment> {
        &self.dealing.coeff_cmt
    }
    fn get_seat_idx(&self) -> usize {
        self.seat_idx
    }
}

pub fn combine_coeff_commitments_shamir<T: HandoverType>(t: usize, handovers: &Vec<T>) -> Vec<G1Affine> {
    let curr_state_pts: Vec<Fr> = handovers
        .iter()
        .map(|handover| Fr::from(handover.get_seat_idx() as u64))
        .collect();
    let lagrange_coeffs = ShamirSecretSharing::lagrange_coeffs(&curr_state_pts, 0);

    let mut bases = Vec::new();
    for i in 0..t {
        let bases_i: Vec<G1Affine> = handovers
            .iter()
            .map(|handover| handover.get_coeff_cmt()[i].cmt.clone())
            .collect();
        bases.push(bases_i);
    }
    let combined_poly = cfg_into_iter_client!(bases).map(|bases_i| {
        <E as Pairing>::G1::msm(&bases_i, &lagrange_coeffs).unwrap().into()
    }).collect::<Vec<G1Affine>>();
    /*
    for i in 0..t {
        let bases: Vec<G1> = handovers
            .iter()
            .map(|handover| handover.get_coeff_cmt()[i].cmt.clone())
            .collect();
        combined_poly.push(
            <E as Pairing>::G1::msm(&bases, &lagrange_coeffs)
                .unwrap()
                .into(),
        );
    }
    */
    combined_poly
}

#[derive(Clone, Serialize, Deserialize)]
pub struct BackupCiphertext {
    pub nonce: Nonce<<Aes256Gcm as AeadCore>::NonceSize>,
    pub ct_aes: Vec<u8>,
    pub ct_ibe: IBECiphertext
}

#[derive(Clone)]
pub struct RecoveryRequest {
    pub req: BlindExtractionRequest,
    pub client_id: usize,
    pub proof: Groth16Proof<E2>,
}

impl Serialize for RecoveryRequest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut bytes = Vec::new();

        bytes.extend_from_slice(&bincode::serialize(&self.req).map_err(serde::ser::Error::custom)?);
        bytes.extend_from_slice(&bincode::serialize(&self.client_id).map_err(serde::ser::Error::custom)?);
        self.proof.serialize_compressed(&mut bytes).unwrap();

        serializer.serialize_bytes(&bytes)
    }
}

impl<'de> Deserialize<'de> for RecoveryRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        let mut cursor = std::io::Cursor::new(bytes);

        // Deserialize each field in order
        let req: BlindExtractionRequest = bincode::deserialize_from(&mut cursor).map_err(de::Error::custom)?;
        let client_id: usize = bincode::deserialize_from(&mut cursor).map_err(de::Error::custom)?;
        let proof = Groth16Proof::<E2>::deserialize_compressed(&mut cursor).unwrap();

        Ok(Self { req, client_id, proof })
    }
}

impl RecoveryRequest {
    pub fn verify(params: &SystemParams, req: &RecoveryRequest) -> Result<bool, ECPSSClientError> {
        let mut input = Vec::new();
        match ToConstraintField::<<P as Bls12Config>::Fp>::to_field_elements(&req.req.req.x) {
            Some(mut x) => input.append(&mut x),
            None => return Err(ECPSSClientError::InvalidRecoveryRequest(InvalidRecoveryRequest::GrothError)),
        };
        match ToConstraintField::<<P as Bls12Config>::Fp>::to_field_elements(&req.req.req.y) {
            Some(mut y) => input.append(&mut y),
            None => return Err(ECPSSClientError::InvalidRecoveryRequest(InvalidRecoveryRequest::GrothError)),
        };
        let id_bytes = req.client_id.to_le_bytes().to_vec();
        let padded_id = pad_to_length(&id_bytes, params.id_len);
        match ToConstraintField::<<P as Bls12Config>::Fp>::to_field_elements(&padded_id) {
            Some(mut id) => input.append(&mut id),
            None => return Err(ECPSSClientError::InvalidRecoveryRequest(InvalidRecoveryRequest::GrothError)),
        };
        match <Groth16<E2> as SNARK::<<P as Bls12Config>::Fp>>::verify(
            &params.groth_vk,
            &input,
            &req.proof
        ) {
            Ok(_) => Ok(true),
            Err(_) => return Err(ECPSSClientError::InvalidRecoveryRequest(InvalidRecoveryRequest::GrothError)),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct RecoveryRequestBatch {
    // recovery request with previous guess attempts
    pub requests: Vec<(RecoveryRequest, usize)>,
    // use the same Merkle path for both proofs
    pub guess_limit_update_proof: UpdateProof<MerkleTreeAVDGuessAttemptsParameters>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct RecoveryResponseBatch {
    pub client_id: usize,
    pub seat_idx: usize,
    pub responses: Vec<BlindExtractionResponse>,
}

pub fn usize_to_bytes_for_avd(id: usize) -> [u8; 32] {
    let mut buf = [0u8; 32];
    buf[..8].copy_from_slice(&id.to_le_bytes());
    buf
}

pub fn bytes_to_usize_for_avd(buf: &[u8; 32]) -> usize {
    let mut num_bytes = [0u8; 8];
    num_bytes.copy_from_slice(&buf[0..8]);
    usize::from_le_bytes(num_bytes)
}

#[derive(Clone, Serialize, Deserialize)]
pub struct RecoveryResponse {
    pub client_id: usize,
    pub rsp: BlindExtractionResponse,
    pub ctxt: BackupCiphertext,
}