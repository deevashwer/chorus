use std::collections::{VecDeque, HashMap};
use std::iter::zip;
use std::ops::Mul;

use ark_bls12_377::{G1Affine, G2Affine};
use ark_ec::bls12::Bls12;
use ark_std::cfg_into_iter;
use class_group::{CL_HSM_Ciphertext, CL_HSM_PublicKey};
use ed25519_dalek::VerifyingKey;
use indicatif::{ProgressBar, ProgressStyle};
use rand::thread_rng;

use crate::crypto::avd::sparse_merkle_tree::{hash_leaf, SparseMerkleTree};
use crate::crypto::nivss;
use crate::crypto::nivss::sa_nivss::DealingLite;
use crate::crypto::proofs::{ChaumPedersen, DLEQInstance, FeldmanCommitment};
use crate::crypto::sortition::SortitionState;
use crate::crypto::{nivss::sa_nivss::NIVSS, shamir::ShamirSecretSharing};
use crate::crypto::avd::{MerkleTreeAVD, MerkleTreeAVDParameters, SingleStepAVD, sparse_merkle_tree::FixedLengthCRH};
use crate::secret_recovery::client::ECPSSClient;
use crate::secret_recovery::error::InvalidState;

use crate::crypto::ibe::{Blind, BonehFranklinIBE, CipherText as IBECipherText, MasterSecretKey, PublicKey as IBEPublicKey, SecretKey as IBESecretKey, BlindExtractionRequest, BlindExtractionResponse};
use crate::secret_recovery::common::{
    combine_coeff_commitments_shamir, seed_hash, CoefficientCommitments, HandoverLite, RecoveryRequest, RecoveryResponse
};

use super::client::{self, check_all_entries_are_distinct};
use super::common::{
    compute_share_commitments, usize_to_bytes_for_avd, bytes_to_usize_for_avd, BackupCiphertext, ClientListMerkleTreeAVD, CommitteeData, CommitteeShareCommitment, CommitteeStateClient, CommitteeStateServer, ECPSSClientData, Fr, GuessAttemptsMerkleTreeAVD, Handover, PublicState, RecoveryRequestBatch, RecoveryResponseBatch, SortitionProof, SystemParams, E, H
};
use ark_ec::{pairing::Pairing, VariableBaseMSM};
use ark_ff::{One, Zero};
use ark_crypto_primitives::snark::SNARK;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

pub struct ServerState<'a> {
    pub params: SystemParams<'a>,
    pub clients: Vec<ECPSSClientData>,
    pub committees: Vec<CommitteeData>,
    pub states: Vec<CommitteeStateServer>,
    pub share_cmts: Vec<Vec<CommitteeShareCommitment>>,
    pub guess_attempts_mt: GuessAttemptsMerkleTreeAVD,
    pub client_mt: ClientListMerkleTreeAVD,
    // pub client_mt: MerkleTree<Sha256>,
    pub epoch: usize,
    pub backup_keys: HashMap<usize, BackupCiphertext>,
}

impl<'a> ServerState<'a> {
    pub fn new(params: SystemParams<'a>) -> Self {
        ServerState {
            params,
            clients: Vec::new(),
            committees: Vec::new(),
            states: Vec::new(),
            share_cmts: Vec::new(),
            guess_attempts_mt: GuessAttemptsMerkleTreeAVD::new(&mut thread_rng(), &()).unwrap(),
            client_mt: ClientListMerkleTreeAVD::new(&mut thread_rng(), &()).unwrap(),
            // client_mt: MerkleTree::<Sha256>::new(),
            epoch: 0,
            backup_keys: HashMap::new(),
        }
    }

    pub fn register_clients(&mut self, client: Vec<ECPSSClientData>) {
        self.clients = client;

        // construct merkle tree
        /*
        let leaves: Vec<[u8; 32]> = (0..self.params.num_clients)
            .map(|i| {
                rs_merkle_hash(&self.clients[i].to_bytes_for_hashing())

            })
            .collect();
        */
        let pb = ProgressBar::new(self.clients.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} ({eta})")
                .progress_chars("#>-"),
        );
        (0..self.params.num_clients).for_each(|i| {
            pb.set_position(i as u64);
            let key = usize_to_bytes_for_avd(self.clients[i].id);
            let value = hash_leaf::<H>(&(), &self.clients[i].to_bytes_for_hashing()).unwrap();
            self.client_mt.update(&key, &value).unwrap();
        });
        pb.finish_with_message("Done!");
    }

    pub fn reset_state_except_clients(&mut self) {
        self.committees.clear();
        self.states.clear();
        self.share_cmts.clear();
        self.guess_attempts_mt = GuessAttemptsMerkleTreeAVD::new(&mut thread_rng(), &()).unwrap();
        self.client_mt = ClientListMerkleTreeAVD::new(&mut thread_rng(), &()).unwrap();
        self.epoch = 0;
        self.backup_keys.clear();
    }

    pub fn get_root(&self) -> Vec<u8> {
        let root: [u8; 32] = self.client_mt.digest().unwrap().clone();
        root.to_vec()
    }

    fn get_seed(&self) -> Vec<u8> {
        let root = self.get_root();
        seed_hash(&root, self.epoch).into()
    }

    pub fn get_committee(&self, idx: usize) -> CommitteeData {
        assert!(self.committees.len() > idx);
        let committee = self.committees[idx].clone();
        committee
    }

    pub fn process_committee(&mut self, sortition_proofs: &Vec<SortitionProof>) {
        let seed = self.get_seed();
        let members: Vec<(ECPSSClientData, SortitionProof)> = cfg_into_iter!(sortition_proofs)
            .filter_map(|sortition_proof| {
                let mut srt_state =
                    SortitionState::new().unwrap();
                let srt_output = &sortition_proof.srt_output;
                let client = &self.clients[sortition_proof.client_id];
                if srt_output.success {
                    let vrf_proof = srt_output.proof.as_ref().unwrap();
                    match srt_state.verify(&seed, &client.vrf_pubkey, &vrf_proof, self.params.num_clients, self.params.committee_size) {
                        Ok(true) => Some((client.clone(), sortition_proof.clone())),
                        Ok(false) => {
                            println!("Proof failed: Hash mismatch");
                            None
                        }
                        Err(e) => {
                            println!("Proof failed: {}", e);
                            None
                        },
                    }
                } else {
                    None
                }
            })
            .collect();
        assert!(members.len() >= self.params.threshold as usize);
        let mut members_indices: Vec<usize> = Vec::new();
        members.iter().for_each(|c| {
            members_indices.push(c.0.id);
        });
        // let merkle_proof = self.client_mt.proof(&members_indices);
        let merkle_proof = members_indices.iter().map(|idx| {
            let key = usize_to_bytes_for_avd(self.clients[*idx].id);
            let (_, _, proof) = self.client_mt.lookup(&key).unwrap();
            proof
        }).collect();
        self.committees.push(CommitteeData {
            epoch: self.epoch,
            root: self.get_root(),
            members,
            merkle_proof,
        });
    }

    pub fn get_state(&self, epoch: usize) -> Vec<CommitteeStateClient> {
        assert!(epoch <= self.states.len());
        // we start processing states from epoch 1
        let state = &self.states[epoch-1];
        // C_{e} processes this state in epoch e + 1
        let committee_to_process_this_state = &self.committees[epoch];
        let n = committee_to_process_this_state.members.len();
        let t = state.handovers.len();

        let nivss = NIVSS::new(&self.params.g1_gen, &self.params.cl_params);
        let lite_dealings: Vec<Vec<DealingLite>> = state.handovers.iter().map(|handover| {
            nivss.get_lite_dealings(&handover.dealing)
        }).collect();
        (0..n).map(|idx| {
            let handovers: Vec<HandoverLite> = (0..t).map(|hnd| {
                let handover = &state.handovers[hnd];
                HandoverLite {
                    epoch: handover.epoch,
                    client_id: handover.client_id,
                    seat_idx: handover.seat_idx,
                    dealing: lite_dealings[hnd][idx].clone(),
                    dleq_proof: handover.dleq_proof.clone(),
                }
            }).collect();
            CommitteeStateClient {
                epoch: state.epoch,
                handovers,
                public_state: state.public_state.clone(),
                coeff_cmts: state.coeff_cmts.clone(),
            }
        }).collect::<Vec<CommitteeStateClient>>()
    }

    pub fn get_master_pk(&self) -> IBEPublicKey {
        assert!(self.epoch >= 2);
        let state = &self.states[1];
        let coeff_cmt = combine_coeff_commitments_shamir(1, &state.handovers).pop().unwrap();
        IBEPublicKey { pk: coeff_cmt }
    }

    pub fn get_public_state(&self) -> PublicState {
        let root: [u8; 32] = self.guess_attempts_mt.digest().unwrap();
        PublicState {
            merkle_root: root.to_vec(),
        }
    }

    // TODO: check for duplicates
    pub fn process_state(&mut self, handovers: &Vec<Handover>) {
        assert!(self.epoch >= 1);

        let next_committee_public_keys: Vec<CL_HSM_PublicKey> = self.committees[self.epoch]
            .members
            .iter()
            .map(|(cl, _)| cl.pke_pubkey.clone())
            .collect();
        let n = next_committee_public_keys.len();
        let curr_g = if self.epoch == 1 {
            &self.params.g1_genprime
        } else {
            &self.params.g1_gen
        };
        let nivss = NIVSS::new(curr_g, &self.params.cl_params);
        let curr_public_state = self.get_public_state();
        // we start processing committees from epoch 0 but we want the previous committee because committee_{e-1} processes state in epoch e
        let n = self.committees[self.epoch - 1].members.len();
        let prev_committee_sig_pubkeys: Vec<VerifyingKey> = self.committees[self.epoch - 1].members.iter().map(|(cl, _)| cl.sig_pubkey.clone()).collect();
        let prev_share_cmts = match self.epoch {
            1 => None,
            _ => {
                let t = self.params.threshold;
                // we start processing states from epoch 1
                // let prev_state = self.states[self.epoch - 2].clone();
                // let coeff_cmts = combine_coeff_commitments_shamir(t, &prev_state.handovers);
                let coeff_cmts = self.states[self.epoch - 2].coeff_cmts.clone();
                // remove the wrapper
                let coeff_cmts = coeff_cmts.coeff_cmts.iter().map(|cmt| cmt.cmt.clone()).collect();
                let indices = (0..=n).collect();
                let share_cmts = compute_share_commitments(t, &indices, &coeff_cmts);
                self.share_cmts.push(share_cmts.clone());
                Some(share_cmts)
            },
        };
        let valid_handovers: Vec<Handover> = cfg_into_iter!(handovers)
            .filter_map(|handover| {
                if handover.epoch != self.epoch {
                    return None;
                };

                // verify coeff_cmts w.r.t. share_cmts of the previous committee
                let cmt_check = if prev_share_cmts.is_some() {
                    let prev_share_cmts = prev_share_cmts.as_ref().unwrap();
                    let seat_idx = handover.seat_idx;
                    // seat_idx start from 1
                    let prev_key = &prev_share_cmts[seat_idx];
                    let curr_key = &handover.dealing.coeff_cmt[0];
                    if handover.dleq_proof.is_some() {
                        assert!(self.epoch == 2);
                        let dleq = ChaumPedersen::new(&self.params.g1_gen, &self.params.g1_genprime);
                        let dleq_proof = handover.dleq_proof.as_ref().unwrap();
                        let instance = DLEQInstance {
                            comm1: curr_key.clone(),
                            comm2: prev_key.clone(),
                        };
                        match dleq.verify(&instance, dleq_proof) {
                            Ok(_) => Ok(()),
                            Err(err) => {
                                Err(InvalidState::DLEQProof(err))
                            }
                        }
                    } else {
                        if prev_key != curr_key {
                            Err(InvalidState::ShareCommitmentsMismatch)
                        } else {
                            Ok(())
                        }
                    }
                } else {
                    Ok(())
                };
                if cmt_check.is_err() {
                    println!("Commitment check failed");
                    return None;
                }

                let seat_idx = handover.seat_idx;
                // verify dealing
                let mut sid = format!("HANDOVER-{}-{}", self.epoch, seat_idx).into_bytes();
                sid.extend_from_slice(&curr_public_state.to_bytes());
                let dealing_check = nivss.verify_dealing(&sid, &handover.dealing, self.params.threshold, &next_committee_public_keys, &prev_committee_sig_pubkeys[seat_idx-1]);
                if dealing_check.is_err() {
                    println!("Dealing check failed: {:?}", dealing_check);
                    return None;
                }

                Some(handover.clone())
            })
            .collect();
        assert!(valid_handovers.len() >= self.params.threshold);
        let valid_handovers = valid_handovers[0..self.params.threshold].to_vec();
        let coeff_cmts = combine_coeff_commitments_shamir(self.params.threshold, &valid_handovers);
        let coeff_cmts = coeff_cmts.iter().map(|cmt| FeldmanCommitment { cmt: cmt.clone() }).collect();

        self.states.push(CommitteeStateServer {
            epoch: self.epoch,
            handovers: valid_handovers,
            public_state: curr_public_state.clone(),
            coeff_cmts: CoefficientCommitments { coeff_cmts },
        });
    }

    pub fn store_backup(&mut self, client_id: usize, backup_ctxt: &BackupCiphertext) {
        self.backup_keys.insert(client_id, backup_ctxt.clone());
    }

    pub fn process_recovery_requests(&mut self, reqs: &Vec<RecoveryRequest>) -> RecoveryRequestBatch {
        let verified_reqs: Vec<RecoveryRequest> = cfg_into_iter!(reqs).filter_map(|req| {
            match RecoveryRequest::verify(&self.params, req) {
                Ok(_) => Some(req.clone()),
                Err(_) => None,
            }
        }).collect();
        // check validity of requests
        let mut valid_reqs = HashMap::new();
        verified_reqs.iter().for_each(|req| {
            // check for duplicates
            match valid_reqs.get(&req.client_id) {
                Some(_) => { 
                    println!("Invalid request: Duplicate");
                    return
                },
                None => (),
            }
            // check that a corresponding ciphertext exists
            match self.backup_keys.get(&req.client_id) {
                Some(_) => {},
                None => {
                    println!("Invalid request: No backup found for client");
                    return
                },
            }
            // check for guess limit
            // todo
            let (val, _, _) = self.guess_attempts_mt.lookup(&usize_to_bytes_for_avd(req.client_id)).unwrap();
            match val {
                Some((_, attempts)) => {
                    let attempts_usize = bytes_to_usize_for_avd(&attempts);
                    if attempts_usize < self.params.guess_limit {
                        valid_reqs.insert(req.client_id, (req.clone(), attempts_usize));
                    } else {
                        println!("Invalid request: Guess limit exceeded");
                    }
                },
                // no previous attempts
                None => {
                    valid_reqs.insert(req.client_id, (req.clone(), 0));
                },
            };
        });
        let prev_digest= self.guess_attempts_mt.digest().unwrap();
        // println!("old merkle proof: {:?}", old_merkle_proof);

        let new_leaves: Vec<([u8; 32], [u8; 32])> = valid_reqs.iter().map(|(id, (_, attempts))| {
            let key = usize_to_bytes_for_avd(*id);
            let value = usize_to_bytes_for_avd(*attempts + 1);
            // println!("id: {:?}, attempts: {:?}, key: {:?}, value: {:?}", id, attempts, key, value);
            (key, value)
        }).collect();
        let (new_digest, new_merkle_proof) = self.guess_attempts_mt.batch_update(&new_leaves).unwrap();
        // println!("new merkle proof: {:?}", new_merkle_proof);
        assert!(GuessAttemptsMerkleTreeAVD::verify_update(&(), &prev_digest, &new_digest, &new_merkle_proof).unwrap());

        let requests = valid_reqs.into_iter().map(|(_, (req, attempts))| (req, attempts)).collect();

        RecoveryRequestBatch { requests, guess_limit_update_proof: new_merkle_proof }
    }

    pub fn process_recovery_responses(&self, rsps: &Vec<RecoveryResponseBatch>, reqs: &RecoveryRequestBatch) -> Vec<RecoveryResponse> {
        assert!(self.epoch >= 3);
        let share_cmts = &self.share_cmts[self.epoch - 2];
        let valid_rsps: Vec<RecoveryResponseBatch> = cfg_into_iter!(rsps).filter_map(|rsp| {
            let mut valid = true;
            if rsp.responses.len() != reqs.requests.len() {
                return None;
            }
            rsp.responses.iter().zip(reqs.requests.iter()).for_each(|(rsp_i, (req, _))| {
                let seat_idx = rsp.seat_idx;
                // includes indices 0 to =n
                let share_cmt = &share_cmts[seat_idx];
                match E::pairing(<<Bls12<ark_bls12_377::Config> as Pairing>::G1Affine as Into<<Bls12<ark_bls12_377::Config> as Pairing>::G1Prepared>>::into(self.params.g1_gen), <<Bls12<ark_bls12_377::Config> as Pairing>::G2Affine as Into<<Bls12<ark_bls12_377::Config> as Pairing>::G2Prepared>>::into(rsp_i.rsp)) == E::pairing(<<Bls12<ark_bls12_377::Config> as Pairing>::G1Affine as Into<<Bls12<ark_bls12_377::Config> as Pairing>::G1Prepared>>::into(share_cmt.cmt), <<Bls12<ark_bls12_377::Config> as Pairing>::G2Affine as Into<<Bls12<ark_bls12_377::Config> as Pairing>::G2Prepared>>::into(req.req.req)) {
                    false => {valid = false},
                    true => {},
                };
            });
            if valid {
                return Some(rsp.clone());
            } else {
                None
            }
        }).collect();
        assert!(valid_rsps.len() >= self.params.threshold);
        // only keep the first t valid responses
        let valid_rsps = valid_rsps[0..self.params.threshold].to_vec();
        let num_reqs = reqs.requests.len();
        let pts: Vec<Fr> = valid_rsps.iter().map(|rsp| Fr::from(rsp.seat_idx as u64)).collect();
        let lagrange_coeffs = ShamirSecretSharing::lagrange_coeffs(&pts, 0);
        let recovery_rsps = cfg_into_iter!(0..num_reqs).map(|i| {
            let bases: Vec<G2Affine> = valid_rsps.iter().map(|rsp| rsp.responses[i].rsp).collect();
            let rsp = <E as Pairing>::G2::msm(&bases, &lagrange_coeffs).unwrap().into();
            let rsp = BlindExtractionResponse { rsp };
            let client_id = reqs.requests[i].0.client_id;
            let ctxt = self.backup_keys.get(&client_id).unwrap().clone();
            RecoveryResponse { client_id, rsp, ctxt }
        }).collect::<Vec<RecoveryResponse>>();
        return recovery_rsps;
    }
}
