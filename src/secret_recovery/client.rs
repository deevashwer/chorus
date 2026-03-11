use ark_ec::bls12::Bls12;
use ark_ec::{pairing::Pairing, VariableBaseMSM, bls12::Bls12Config, short_weierstrass::SWCurveConfig, CurveGroup};
use ark_std::{cfg_into_iter, cfg_iter_mut, end_timer, start_timer};
use class_group::{CL_HSM_Ciphertext, CL_HSM_PublicKey, CL_HSM_SecretKey, QFI};
use ed25519_dalek::ed25519::signature::SignerMut;
use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use rand::thread_rng;
use rug::{integer::Order, Integer, rand::RandState};
use block_padding::{Pkcs7, Padding};
use rand::rngs::StdRng;

use super::common::{
    BackupCiphertext, CoefficientCommitments, CommitteeData, CommitteeStateClient, ECPSSClientData, Handover, HandoverLite, PublicState, RecoveryRequest, RecoveryRequestBatch, RecoveryResponse, SortitionProof, SystemParams
};
use super::common::{CommitteeShareCommitment, Fr, E, E2, G1Affine, P, seed_hash};
use super::error::{ECPSSClientError, SecretRecoveryClientError};
use crate::crypto::avd::sparse_merkle_tree::hash_leaf;
use crate::crypto::avd::LookupProof;
use crate::crypto::avd::{SingleStepAVD, sparse_merkle_tree::FixedLengthCRH};
use crate::crypto::nivss::error::NIVSSError;
use crate::crypto::nivss::sa_nivss::{Dealing, DealingLite};
use crate::crypto::proofs::pocs::coeffs_and_shares_consistency_check_exponents;
use crate::crypto::proofs::{CLKoE, CLKoEInstance, CLKoEProof, CLKoEWitness, FeldmanCommitment};
use crate::crypto::shamir::ShamirSecretSharing;
use crate::crypto::ibe::{Blind, BlindExtractionRequest, BlindExtractionResponse, BonehFranklinIBE, CipherText as IBECipherText, PublicKey as IBEPublicKey, SecretKey as IBESecretKey, MasterSecretKey, constraints::RecoveryRequestNIZK, error::IBEError};
use crate::secret_recovery::common::{bytes_to_usize_for_avd, usize_to_bytes_for_avd, ClientListMerkleTreeAVD, GuessAttemptsMerkleTreeAVD, RecoveryResponseBatch, H};
use crate::secret_recovery::error::{InvalidCommittee, InvalidRecoveryRequest, InvalidState};
use crate::{
    crypto::{
        nivss::sa_nivss::NIVSS,
        proofs::{
            msm::cl_msm,
            utils::{field_to_integer, integer_to_field},
            ChaumPedersen, DLEQInstance, DLEQProof, DLEQWitness,
        },
        sortition::{SortitionOutput, SortitionState, VRFPublicKey, VRFSecretKey},
    },
    secret_recovery::common::{
        combine_coeff_commitments_shamir, compute_share_commitments
    },
};
use ark_ff::UniformRand;
use rand::{rngs::OsRng, seq::SliceRandom, SeedableRng, RngCore};
use rand_chacha::ChaCha20Rng;
use std::collections::{HashMap, HashSet};
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit},
    Aes256Gcm, Nonce, Key
};

use ark_crypto_primitives::snark::SNARK;
use ark_groth16::{Groth16, Proof as Groth16Proof};

#[cfg(feature = "parallel")]
use rayon::prelude::*;

#[macro_export]
macro_rules! start_timer_if {
    ($pred:expr, $msg:expr) => {{
        if $pred {
            Some(start_timer!($msg))
        } else {
            None
        }
    }};
}

#[macro_export]
macro_rules! end_timer_if {
    ($timer_opt:expr) => {{
        if let Some(timer) = $timer_opt {
            end_timer!(timer);
        }
    }};
    ($timer_opt:expr, $msg:expr) => {{
        if let Some(timer) = $timer_opt {
            end_timer!(timer, $msg);
        }
    }};
}

#[macro_export]
macro_rules! cfg_into_iter_client {
    ($e: expr, $min_len: expr) => {{
        #[cfg(all(feature = "client-parallel", target_os = "android"))]
        let result = $e.into_par_iter().with_min_len($min_len);

        #[cfg(any(not(feature = "client-parallel"), not(target_os = "android")))]
        let result = $e.into_iter();

        result
    }};
    ($e: expr) => {{
        #[cfg(all(feature = "client-parallel", target_os = "android"))]
        let result = $e.into_par_iter();

        #[cfg(any(not(feature = "client-parallel"), not(target_os = "android")))]
        let result = $e.into_iter();

        result
    }};
}

pub fn check_all_entries_are_distinct<T: Eq + std::hash::Hash + Clone>(items: &Vec<T>) -> bool {
    let mut seen = HashSet::new();
    for item in items {
        if !seen.insert(item) {
            return false; // Duplicate found
        }
    }
    true
}

pub fn pad_to_length(data: &[u8], length: usize) -> Vec<u8> {
    assert!(length >= data.len());
    let mut padded_data = data.to_vec();
    let padding = length - data.len();
    padded_data.extend(vec![0u8; padding]);
    padded_data
}

pub fn verify_share_commitments(
    t: usize,
    coeff_cmts: &Vec<G1Affine>,
    share_cmts: &Vec<G1Affine>,
) -> Result<(), InvalidState> {
    // check the constant term of combined_poly against next_committee_share_cmts[0]
    if coeff_cmts[0] != share_cmts[0] {
        return Err(InvalidState::ShareCommitmentsMismatch)?;
    };
    // get share commitments from the combined polynomial (do a randomized check)
    let n = share_cmts.len() - 1; // skipping the evaluation at 0
    let gamma = <E as Pairing>::ScalarField::rand(&mut rand::thread_rng());
    let (powers_of_gamma, exponents) =
        coeffs_and_shares_consistency_check_exponents::<E>(n, t, gamma);
    let lhs = <E as Pairing>::G1::msm(&coeff_cmts, &exponents).unwrap();
    // next_committee_share_cmts also includes the evaluation at idx 0 (ignoring that)
    let rhs = <E as Pairing>::G1::msm(&share_cmts[1..], &powers_of_gamma).unwrap();
    if lhs != rhs {
        return Err(InvalidState::ShareCommitmentsMismatch)?;
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct CommitteeSeat {
    pub total_seats: usize,
    pub seat_idx: usize,
    pub pke_keys: (CL_HSM_SecretKey, CL_HSM_PublicKey),
    pub sig_keys: SigningKey,
}

pub struct ECPSSClient {
    pub id: usize,
    srt_state: SortitionState,
    vrf_keys: (VRFSecretKey, VRFPublicKey),
    sig_keys: SigningKey,
    pke_keys: (CL_HSM_SecretKey, CL_HSM_PublicKey),
    pke_proof: CLKoEProof,
}

impl ECPSSClient {
    pub fn new(client_idx: usize, params: SystemParams) -> Self {
        let mut srt_state = SortitionState::new().unwrap();
        let vrf_keys = srt_state.keygen(client_idx).unwrap();
        
        #[cfg(feature = "deterministic")]
        let mut csprng = ChaCha20Rng::seed_from_u64(client_idx as u64);
        #[cfg(not(feature = "deterministic"))]
        let mut csprng = OsRng::default();
        let sig_keys = SigningKey::generate(&mut csprng);

        let mut rng = RandState::new();
        if cfg!(feature = "deterministic") {
            rng.seed(&Integer::from(client_idx as i64));
        } else { 
            let mut os_rng = OsRng::default();
            let seed = Integer::from(os_rng.next_u64() as i64);
            rng.seed(&seed);
        };
        let (sk, pk): (CL_HSM_SecretKey, CL_HSM_PublicKey) = params.cl_params.keygen(&mut rng);

        // generate cl_koe proof
        let cl_koe = CLKoE::new(&params.cl_params);
        let instance = CLKoEInstance {
            public_key: pk.clone(),
        };
        let witness = CLKoEWitness {
            secret_key: sk.clone(),
        };
        let koe_proof = cl_koe.prove(&instance, &witness, &mut rand::thread_rng());
        ECPSSClient {
            id: client_idx,
            srt_state,
            vrf_keys,
            sig_keys,
            pke_keys: (sk, pk),
            pke_proof: koe_proof,
        }
    }

    pub fn register(&self) -> ECPSSClientData {
        ECPSSClientData {
            id: self.id,
            pke_pubkey: self.pke_keys.1.clone(),
            sig_pubkey: self.sig_keys.verifying_key(),
            vrf_pubkey: self.vrf_keys.1.clone(),
            pubkey_proof: self.pke_proof.clone(),
        }
    }

    pub fn sortition(&mut self, root: &Vec<u8>, params: &SystemParams, epoch: usize) -> Result<Option<SortitionProof>, ECPSSClientError> {
        let seed: Vec<u8> = seed_hash(&root.to_vec(), epoch).into();

        let srt_output = self.srt_state.eval(&seed, &self.vrf_keys.0, params.num_clients, params.committee_size).unwrap();

        if srt_output.success {
            let client_idx = self.id;

            let srt = SortitionProof {
                epoch: epoch,
                client_id: client_idx,
                srt_output: srt_output.clone(),
            };
            Ok(Some(srt))
        } else {
            Ok(None)
        }
    }

    // verify the committee information committed by the server
    // sometimes we don't need to verify well-formedness of the committee PKE public keys (verify_pke_koe = false)
    pub fn verify_committee(
        &mut self,
        committee: &CommitteeData,
        params: &SystemParams,
        epoch: usize,
        verify_pke_koe: bool,
    ) -> Result<(), InvalidCommittee> {
        let root = &committee.root;
        let seed: Vec<u8> = seed_hash(root, epoch).into();
        let root_bytes: [u8; 32] = root.clone().try_into().unwrap();

        assert!(committee.epoch == epoch);

        // verify Merkle proof
        committee.members.iter().zip(committee.merkle_proof.iter()).map(|((c, _), proof)| {
            let key = usize_to_bytes_for_avd(c.id);
            let value = hash_leaf::<H>(&(), &c.to_bytes_for_hashing()).unwrap();
            match ClientListMerkleTreeAVD::verify_lookup(&(), &key, &Some((0, value)), &root_bytes, proof) {
                Ok(_) => (),
                Err(e) => {
                    return Err(InvalidCommittee::MerkleProof(Some(e)));
                }
            }
            Ok(())
        }).collect::<Result<_, InvalidCommittee>>()?;
        // verify committee size >= threshold
        if committee.members.len() < params.threshold {
            return Err(InvalidCommittee::SmallerThanThreshold);
        }

        // committee should have distinct client ids, distinct public keys, distinct verification keys, and distinct VRF public keys
        let ids: Vec<usize> = committee.members.iter().map(|(c, _)| c.id).collect();
        let ids_distinct = check_all_entries_are_distinct(&ids);
        let pke_pubkeys: Vec<CL_HSM_PublicKey> =
            committee.members.iter().map(|(c, _)| c.pke_pubkey.clone()).collect();
        let pke_pubkeys_distinct = check_all_entries_are_distinct(&pke_pubkeys);
        let sig_pubkeys: Vec<VerifyingKey> =
            committee.members.iter().map(|(c, _)| c.sig_pubkey.clone()).collect();
        let sig_pubkeys_distinct = check_all_entries_are_distinct(&sig_pubkeys);
        let vrf_pubkeys: Vec<VRFPublicKey> =
            committee.members.iter().map(|(c, _)| c.vrf_pubkey.clone()).collect();
        let vrf_pubkeys_distinct = check_all_entries_are_distinct(&vrf_pubkeys);
        if !ids_distinct || !pke_pubkeys_distinct || !sig_pubkeys_distinct || !vrf_pubkeys_distinct {
            return Err(InvalidCommittee::DuplicateEntries);
        }

        // check sortition proofs
        cfg_into_iter_client!(&committee.members).map(|(client, nom)| {
            // verify epoch
            assert!(nom.epoch == epoch);

            // verify sortition proof
            let srt_output = &nom.srt_output;
            assert!(srt_output.success);
            let vrf_proof = srt_output.proof.as_ref().unwrap();
            let mut srt_state = SortitionState::new().unwrap();
            match srt_state.verify(&seed, &client.vrf_pubkey, &vrf_proof, params.num_clients, params.committee_size) {
                Ok(_) => (),
                Err(err) => {
                    return Err(InvalidCommittee::SortitionProof(err));
                }
            }

            if verify_pke_koe {
                // verify KoE proof
                let cl_koe = CLKoE::new(&params.cl_params);
                let instance = CLKoEInstance {
                    public_key: client.pke_pubkey.clone(),
                };
                match cl_koe.verify(&instance, &client.pubkey_proof) {
                    Ok(_) => (),
                    Err(err) => {
                        return Err(InvalidCommittee::Proofs(err));
                    }
                }
            }
            Ok(())
        }).collect::<Result<_, InvalidCommittee>>()?;
        Ok(())
    }

    pub fn on_committee(
        &mut self,
        committee: &CommitteeData,
        params: &SystemParams,
        epoch: usize,
    ) -> Result<Option<CommitteeSeat>, InvalidCommittee> {
        let client_ids = committee
            .members
            .iter()
            .map(|(c, _)| c.id)
            .collect::<Vec<usize>>();
        let committee_idx = client_ids.iter().position(|id| id == &self.id);
        if committee_idx.is_some() {
            self.verify_committee(committee, params, epoch, false)?;
            let total_seats = committee.members.len();
            let seat_idx = committee_idx.unwrap() + 1;
            let pke_keys = self.pke_keys.clone();
            let sig_keys = self.sig_keys.clone();
            Ok(Some(CommitteeSeat { total_seats, seat_idx, pke_keys, sig_keys }))
        } else {
            Ok(None)
        }
    }

    pub fn retrieve_committee_pkekeys(
        &mut self,
        committee: &CommitteeData,
        params: &SystemParams,
        epoch: usize,
    ) -> Result<Vec<CL_HSM_PublicKey>, InvalidCommittee> {
        self.verify_committee(committee, params, epoch, true)?;
        Ok(committee
            .members
            .iter()
            .map(|(cl, _)| cl.pke_pubkey.clone())
            .collect())
    }

    pub fn retrieve_committee_sigkeys(
        &mut self,
        committee: &CommitteeData,
        params: &SystemParams,
        epoch: usize,
    ) -> Result<Vec<VerifyingKey>, InvalidCommittee> {
        self.verify_committee(committee, params, epoch, false)?;
        Ok(committee
            .members
            .iter()
            .map(|(cl, _)| cl.sig_pubkey.clone())
            .collect())
    }

    pub fn verify_consistency_between_states(
        prev_coeff_cmts: &CoefficientCommitments,
        curr_state: &CommitteeStateClient,
        params: &SystemParams,
    ) -> Result<(), InvalidState> {
        let t = params.threshold;
        let pts: Vec<usize> = curr_state
            .handovers
            .iter()
            .map(|handover| handover.seat_idx)
            .collect();
        let prev_coeff_cmts_g1affine = prev_coeff_cmts.coeff_cmts.iter().map(|c| c.cmt.clone()).collect();
        let prev_share_cmts = compute_share_commitments(t, &pts, &prev_coeff_cmts_g1affine);

        cfg_into_iter_client!(&curr_state.handovers).enumerate().map(|(i, handover)| {
            let prev_key = &prev_share_cmts[i];
            let curr_key = &handover.dealing.coeff_cmt[0];
            if handover.dleq_proof.is_some() {
                let dleq = ChaumPedersen::new(&params.g1_gen, &params.g1_genprime);
                let dleq_proof = handover.dleq_proof.as_ref().unwrap();
                let instance = DLEQInstance {
                    comm1: curr_key.clone(),
                    comm2: prev_key.clone(),
                };
                match dleq.verify(&instance, dleq_proof) {
                    Ok(_) => (),
                    Err(err) => {
                        return Err(InvalidState::DLEQProof(err));
                    }
                }
            } else {
                if prev_key != curr_key {
                    return Err(InvalidState::ShareCommitmentsMismatch)?;
                };
            }
            Ok(())
        })
        .collect::<Result<(), InvalidState>>()?;

        let curr_coeff_cmts = combine_coeff_commitments_shamir(t, &curr_state.handovers);
        for i in 0..t {
            if curr_state.coeff_cmts.coeff_cmts[i].cmt != curr_coeff_cmts[i] {
                return Err(InvalidState::CoeffCommitmentsMismatch)?;
            }
        }
        Ok(())
    }

    pub fn process_recovery_requests(&self, share: &<E as Pairing>::ScalarField, seat_idx: usize, reqs: Option<&RecoveryRequestBatch>, old_public_state: &PublicState, new_public_state: &PublicState, params: &SystemParams) -> Result<Option<RecoveryResponseBatch>, ECPSSClientError> {
        if reqs.is_some() {
            let reqs = reqs.unwrap();
            let share_ibe_msk = MasterSecretKey { msk: share.clone() };
            #[cfg(feature = "print-trace")]
            let verify_recovery_nizks = start_timer!(|| "verify recovery request nizks");
            cfg_into_iter_client!(&reqs.requests).map(|(req, attempts)| {
                RecoveryRequest::verify(&params, &req)?;
                if *attempts >= params.guess_limit {
                    return Err(ECPSSClientError::InvalidRecoveryRequest(InvalidRecoveryRequest::TooManyAttempts))?;
                }
                Ok(())
            }).collect::<Result<(), ECPSSClientError>>()?;
            #[cfg(feature = "print-trace")]
            end_timer!(verify_recovery_nizks);
            #[cfg(feature = "print-trace")]
            let verify_merkle_proof = start_timer!(|| "verify merkle proof");
            let client_ids: Vec<usize> = reqs.requests.iter().map(|(req,_)| req.client_id).collect();
            check_all_entries_are_distinct(&client_ids);
            let old_root = old_public_state.to_bytes_for_mt();
            let new_root = new_public_state.to_bytes_for_mt();
            match GuessAttemptsMerkleTreeAVD::verify_update(&(), &old_root, &new_root, &reqs.guess_limit_update_proof).unwrap()
            {
                true => {},
                false => {
                    return Err(ECPSSClientError::InvalidRecoveryRequest(InvalidRecoveryRequest::MerkleProof))?;
                },
            }
            let total_reqs = reqs.requests.len();
            (0..total_reqs).map(|i| {
                let attempts = reqs.requests[i].1;
                let prev_value = bytes_to_usize_for_avd(&reqs.guess_limit_update_proof.prev_values[i]);
                let new_value = bytes_to_usize_for_avd(&reqs.guess_limit_update_proof.new_values[i]);
                match attempts {
                    0 => {
                        if new_value != 1 {
                            return Err(ECPSSClientError::InvalidRecoveryRequest(InvalidRecoveryRequest::MerkleProof))?;
                        }
                    },
                    _ => {
                        if prev_value + 1 != new_value && attempts != prev_value {
                            return Err(ECPSSClientError::InvalidRecoveryRequest(InvalidRecoveryRequest::MerkleProof))?;
                        }
                    }
                }
                Ok(())
            }).collect::<Result<(), ECPSSClientError>>()?;
            #[cfg(feature = "print-trace")]
            end_timer!(verify_merkle_proof);
            #[cfg(feature = "print-trace")]
            let compute_recovery_response = start_timer!(|| "compute recovery response");
            let responses = reqs.requests.iter().map(|(req,_)| {
                match BonehFranklinIBE::blind_extract_response(&req.req, &share_ibe_msk) {
                    Ok(resp) => Ok(resp),
                    Err(e) => Err(ECPSSClientError::InvalidRecoveryRequest(InvalidRecoveryRequest::IBEError(e))),
                }
            }).collect::<Result<Vec<BlindExtractionResponse>, ECPSSClientError>>()?;
            let rsp = RecoveryResponseBatch { client_id: self.id, seat_idx, responses };
            #[cfg(feature = "print-trace")]
            end_timer!(compute_recovery_response);
            Ok(Some(rsp))
        } else {
            if old_public_state != new_public_state {
                return Err(ECPSSClientError::InvalidRecoveryRequest(InvalidRecoveryRequest::RootMismatchWithNoRequests))?;
            } else {
                Ok(None)
            }
        }
    }

    pub fn contribute_dkg_randomness(
        &mut self,
        seat: &CommitteeSeat,
        next_committee: &CommitteeData,
        params: &SystemParams,
        epoch: usize,
    ) -> Result<Option<Handover>, ECPSSClientError> {
        assert!(epoch == 1);

        // verify committee
        let next_committee_pkekeys = self.retrieve_committee_pkekeys(next_committee, params, epoch)?;

        let mut rng = rand::thread_rng();
        let share = <E as Pairing>::ScalarField::rand(&mut rng);
        let init_public_state = PublicState { merkle_root: GuessAttemptsMerkleTreeAVD::new(&mut thread_rng(), &()).unwrap().digest().unwrap().to_vec() };
        // use g1_genprime while contributing
        let nivss = NIVSS::new(&params.g1_genprime, &params.cl_params);
        let mut sid = format!("HANDOVER-{}-{}", epoch, seat.seat_idx).into_bytes();
        sid.extend_from_slice(&init_public_state.to_bytes());
        let dealing = nivss.deal(
            &sid,
            &share,
            params.threshold,
            &next_committee_pkekeys,
            &mut self.sig_keys,
            &mut rng,
        );
        let handover = Handover { epoch, client_id: self.id, seat_idx: seat.seat_idx, dealing, dleq_proof: None };
        Ok(Some(handover))
    }

    pub fn handover(
        &mut self,
        seat: &CommitteeSeat,
        new_public_state: &PublicState,
        prev_committee: &CommitteeData,
        next_committee: &CommitteeData,
        prev_state: Option<&CoefficientCommitments>,
        curr_state: &CommitteeStateClient,
        reqs: Option<&RecoveryRequestBatch>,
        params: &SystemParams,
        epoch: usize,
    ) -> Result<Option<(Handover, Option<RecoveryResponseBatch>)>, ECPSSClientError> {
        assert!(epoch >= 2);

        // verify committees
        #[cfg(feature = "print-trace")]
        let next_committee_timer = start_timer!(|| "verify next committee");
        let next_committee_pkekeys = self.retrieve_committee_pkekeys(next_committee, params, epoch)?;
        #[cfg(feature = "print-trace")]
        end_timer!(next_committee_timer);
        #[cfg(feature = "print-trace")]
        let prev_committee_timer = start_timer!(|| "verify prev committee");
        let prev_committee_sigkeys = self.retrieve_committee_sigkeys(prev_committee, params, epoch - 2)?;
        #[cfg(feature = "print-trace")]
        end_timer!(prev_committee_timer);

        let (prev_g, curr_g) = if epoch == 2 {
            (params.g1_genprime, params.g1_gen)
        } else {
            (params.g1_gen, params.g1_gen)
        };
        // curr_state was setup in the previous epoch (epoch - 1)
        if prev_state.is_some() {
            #[cfg(feature = "print-trace")]
            let verify_consistency_timer = start_timer!(|| "verify consistency between states");
            ECPSSClient::verify_consistency_between_states(prev_state.unwrap(), curr_state, params)?;
            #[cfg(feature = "print-trace")]
            end_timer!(verify_consistency_timer);
        } else {
            assert!(epoch == 2);
        }

        #[cfg(feature = "print-trace")]
        let receive_shares_timer = start_timer!(|| "receive shares");
        let old_public_state = &curr_state.public_state;
        let nivss = NIVSS::new(&prev_g, &params.cl_params);
        let shares_and_seats = cfg_into_iter_client!(&curr_state.handovers).map(|handover| {
                let handover_seat_idx = handover.seat_idx;
                let mut sid = format!("HANDOVER-{}-{}", epoch-1, handover_seat_idx).into_bytes();
                sid.extend_from_slice(&old_public_state.to_bytes());
                let client_seat_idx = seat.seat_idx;
                // share index starts from 0 (hence seat_idx - 1)
                let share = match nivss.receive(&sid, &handover.dealing, params.threshold, &prev_committee_sigkeys[handover_seat_idx - 1], client_seat_idx-1, &self.pke_keys.0, None) {
                    Ok(share) => share,
                    Err(e) => return Err(InvalidState::NIVSS(e)),
                };
                Ok((share, handover.seat_idx))
            })
            .collect::<Result<Vec<(<E as Pairing>::ScalarField, usize)>, InvalidState>>()?;
        // reconstruct share of msk
        let (shares, pts): (Vec<<E as Pairing>::ScalarField>, Vec<<E as Pairing>::ScalarField>) = shares_and_seats.iter().map(|(share, seat_idx)| (*share, <E as Pairing>::ScalarField::from(*seat_idx as u64))).unzip();
        let share = ShamirSecretSharing::recover_secret(&shares, Some(&pts));
        #[cfg(feature = "print-trace")]
        end_timer!(receive_shares_timer);

        // handle recovery requests
        #[cfg(feature = "print-trace")]
        let process_requests_timer = start_timer!(|| "process_requests");
        let rsp = self.process_recovery_requests(&share, seat.seat_idx, reqs, &old_public_state, new_public_state, params)?;
        #[cfg(feature = "print-trace")]
        end_timer!(process_requests_timer);

        #[cfg(feature = "print-trace")]
        let reshare_timer = start_timer!(|| "reshare");
        let mut rng = rand::thread_rng();
        let nivss = NIVSS::new(&curr_g, &params.cl_params);
        let mut sid = format!("HANDOVER-{}-{}", epoch, seat.seat_idx).into_bytes();
        sid.extend_from_slice(&new_public_state.to_bytes());
        let dealing = nivss.deal(
            &sid,
            &share,
            params.threshold,
            &next_committee_pkekeys,
            &mut self.sig_keys,
            &mut rng,
        );
        #[cfg(feature = "print-trace")]
        end_timer!(reshare_timer);

        // create dleq proof if prev_g and curr_g differ
        #[cfg(feature = "print-trace")]
        let compute_dleq_proof = start_timer!(|| "compute dleq_proof");
        let dleq_proof: Option<DLEQProof> = if prev_g != curr_g {
            let dleq = ChaumPedersen::new(&curr_g, &prev_g);
            let comm1 = CommitteeShareCommitment::new(&curr_g, &share);
            let comm2 = CommitteeShareCommitment::new(&prev_g, &share);
            let instance = DLEQInstance { comm1, comm2 };
            let witness = DLEQWitness {
                opening: share.clone(),
            };
            let proof = dleq.prove(&instance, &witness, &mut rand::thread_rng());
            Some(proof)
        } else {
            None
        };
        #[cfg(feature = "print-trace")]
        end_timer!(compute_dleq_proof);

        let handover = Handover { epoch, client_id: self.id, seat_idx: seat.seat_idx, dealing, dleq_proof };

        Ok(Some((handover, rsp)))
    }
}

pub struct ECPSSClientsPool<'a> {
    pub params: SystemParams<'a>,
    pub states: Vec<ECPSSClient>,
    pub epoch: usize,
}

impl<'a> ECPSSClientsPool<'a> {
    pub fn new(params: SystemParams<'a>) -> Self {
        let states = cfg_into_iter!(0..params.num_clients).map(|i| {
            ECPSSClient::new(i, params.clone())
        }).collect::<Vec<_>>();
        ECPSSClientsPool {
            params,
            states,
            epoch: 0,
        }
    }

    pub fn register(&self) -> Vec<ECPSSClientData> {
        let mut clients = Vec::new();
        for state in &self.states {
            clients.push(state.register());
        }
        clients
    }

    pub fn sortition(&mut self, root: &Vec<u8>) -> Vec<SortitionProof> {
        let sortition_proofs: Vec<SortitionProof> = cfg_iter_mut!(self.states)
            .filter_map(|state| {
                match state.sortition(root, &self.params, self.epoch) {
                    Ok(srt) => srt,
                    Err(_) => None,
                }
            })
            .collect();
        sortition_proofs
    }

    pub fn contribute_dkg_randomness(
        &mut self,
        curr_committee: &CommitteeData,
        next_committee: &CommitteeData,
    ) -> Vec<Handover> {
        assert!(self.epoch == 1);

        let handovers = cfg_iter_mut!(self.states)
            .filter_map(|state| {
                let seat = match state.on_committee(curr_committee, &self.params, self.epoch - 1) {
                    Ok(s) => s,
                    Err(e) => {
                        println!("Error: {:?}", e);
                        None
                    },
                };
                if seat.is_none() {
                    return None;
                }
                let seat = seat.unwrap();
                match state.contribute_dkg_randomness(
                    &seat,
                    next_committee,
                    &self.params,
                    self.epoch,
                ) {
                    Ok(h) => h,
                    Err(e) => {
                        println!("Seat: {:?}, Error: {:?}", seat.seat_idx, e);
                        None
                    },
                }
            })
            .collect();
        handovers
    }

    pub fn handover(
        &mut self,
        new_public_state: &PublicState,
        prev_committee: &CommitteeData,
        curr_committee: &CommitteeData,
        next_committee: &CommitteeData,
        prev_state: Option<&CommitteeStateClient>,
        curr_state: &Vec<CommitteeStateClient>,
        reqs: Option<&RecoveryRequestBatch>,
    ) -> Vec<(Handover, Option<RecoveryResponseBatch>)> {
        assert!(self.epoch >= 2);
        let prev_coeff_cmts = match prev_state {
            Some(s) => Some(&s.coeff_cmts),
            None => {
                None
            }
        };
        let handovers = cfg_iter_mut!(self.states)
            .filter_map(|state| {
                let seat = match state.on_committee(curr_committee, &self.params, self.epoch - 1) {
                    Ok(s) => s,
                    Err(_) => None,
                };
                if seat.is_none() {
                    return None;
                }
                let seat = seat.unwrap();
                match state.handover(
                    &seat,
                    new_public_state,
                    prev_committee,
                    next_committee,
                    prev_coeff_cmts,
                    &curr_state[seat.seat_idx - 1],
                    reqs,
                    &self.params,
                    self.epoch,
                ) {
                    Ok(result) => {
                        result
                    },
                    Err(e) => {
                        println!("Error: {:?}", e);
                        None
                    }
                }
            })
            .collect();
        handovers
    }

    pub fn get_master_pk(committee_1: &CommitteeData, commstate_1: &CommitteeStateClient) -> Result<IBEPublicKey, ECPSSClientError> {
        let coeff_cmt = combine_coeff_commitments_shamir(1, &commstate_1.handovers).pop().unwrap();
        Ok(IBEPublicKey { pk: coeff_cmt })
    }
}

pub struct SecretRecoveryClient<'a> {
    params: SystemParams<'a>,
    pk: IBEPublicKey,
    id: usize,
}

impl<'a> SecretRecoveryClient<'a> {
    pub fn new(id: usize, pk: &IBEPublicKey, params: SystemParams<'a>) -> Self {
        SecretRecoveryClient {
            params, 
            pk: pk.clone(),
            id,
        }
    }

    pub fn backup(&self, pwd: &Vec<u8>, secret: &Vec<u8>) -> Result<BackupCiphertext, SecretRecoveryClientError> {
        let id_bytes = self.id.to_le_bytes().to_vec();
        let padded_id = pad_to_length(&id_bytes, self.params.id_len);
        let padded_pwd = pad_to_length(&pwd, self.params.pwd_len);
        let ibe_id: Vec<u8> = padded_id.into_iter().chain(padded_pwd.into_iter()).collect::<Vec<_>>();

        let key = Aes256Gcm::generate_key(OsRng);
        let key_bytes = key.to_vec();
        // println!("Key: {:?}", key);
        // println!("Key bytes len: {}, Key bytes {:?}", key_bytes.len(), key_bytes);
        let cipher = Aes256Gcm::new(&key);
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng); // 96-bits; unique per message
        let ct_aes = cipher.encrypt(&nonce, &secret[..])?;

        #[cfg(feature = "deterministic")]
        let mut rng = ChaCha20Rng::seed_from_u64(self.id as u64);
        #[cfg(not(feature = "deterministic"))]
        let mut rng = rand::thread_rng();
        let ct_ibe = BonehFranklinIBE::encrypt(&self.pk, &ibe_id, &key_bytes, &mut rng)?;
        Ok(BackupCiphertext { nonce, ct_aes, ct_ibe })
    }

    pub fn recovery_request(&self, pwd: &Vec<u8>) -> Result<(RecoveryRequest, Blind), SecretRecoveryClientError> {
        let id_bytes = self.id.to_le_bytes().to_vec();
        let padded_id = pad_to_length(&id_bytes, self.params.id_len);
        let padded_pwd = pad_to_length(&pwd, self.params.pwd_len);
        let ibe_id: Vec<u8> = padded_id.iter().chain(padded_pwd.iter()).map(|id| *id).collect::<Vec<_>>();
        // generate blind extraction request
        #[cfg(feature = "deterministic")]
        let mut rng = ChaCha20Rng::seed_from_u64(self.id as u64);
        #[cfg(not(feature = "deterministic"))]
        let mut rng = rand::thread_rng();
        let (req, blind) = BonehFranklinIBE::blind_extract_request(&ibe_id, &mut rng)?;
        // generate nizk to make it partially blind
        let nizk_prove = RecoveryRequestNIZK::<P> {
            request: Some(req.req.clone()),
            client_id: Some(padded_id),
            pwd: Some(padded_pwd),
            blind: Some(blind.blind),
            id_len: self.params.id_len,
            pwd_len: self.params.pwd_len,
        };
        let proof =
            <Groth16<E2> as SNARK<<P as Bls12Config>::Fp>>::prove(&self.params.groth_pk, nizk_prove, &mut rng).unwrap();
        Ok((RecoveryRequest { req, client_id: self.id, proof }, blind))
    }

    pub fn recover(&self, rsp: &RecoveryResponse, blind: &Blind, pwd: &Vec<u8>) -> Result<Vec<u8>, SecretRecoveryClientError> {
        let sk = BonehFranklinIBE::blind_extract(&rsp.rsp, blind).unwrap();

        // /*
        // sanity check
        let id_bytes = self.id.to_le_bytes().to_vec();
        let padded_id = pad_to_length(&id_bytes, self.params.id_len);
        let padded_pwd = pad_to_length(&pwd, self.params.pwd_len);
        let ibe_id: Vec<u8> = padded_id.iter().chain(padded_pwd.iter()).map(|id| *id).collect::<Vec<_>>();
        let hashed_id = BonehFranklinIBE::h1(&ibe_id).expect("id hash failed");
        let lhs = E::pairing(<<Bls12<ark_bls12_377::Config> as Pairing>::G1Affine as Into<<Bls12<ark_bls12_377::Config> as Pairing>::G1Prepared>>::into(self.params.g1_gen), <<Bls12<ark_bls12_377::Config> as Pairing>::G2Affine as Into<<Bls12<ark_bls12_377::Config> as Pairing>::G2Prepared>>::into(sk.sk));
        let rhs = E::pairing(<<Bls12<ark_bls12_377::Config> as Pairing>::G1Affine as Into<<Bls12<ark_bls12_377::Config> as Pairing>::G1Prepared>>::into(self.pk.pk), <<Bls12<ark_bls12_377::Config> as Pairing>::G2Affine as Into<<Bls12<ark_bls12_377::Config> as Pairing>::G2Prepared>>::into(hashed_id));
        assert_eq!(lhs, rhs); 
        // */
        
        let key: [u8; 32] = BonehFranklinIBE::decrypt(&self.pk, &ibe_id, &sk, &rsp.ctxt.ct_ibe)?.try_into().unwrap();
        let key = Key::<Aes256Gcm>::from_slice(&key);
        let cipher = Aes256Gcm::new(&key);
        let secret = cipher.decrypt(&rsp.ctxt.nonce, rsp.ctxt.ct_aes.as_ref())?;

        Ok(secret)
    }

}
