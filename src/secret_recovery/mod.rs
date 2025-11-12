use core::num;

use self::client::ECPSSClientsPool;
use self::{common::SystemParams, server::ServerState};
use ark_std::{end_timer, start_timer};
use common::{RecoveryRequest, RecoveryRequestBatch, RecoveryResponse, RecoveryResponseBatch};

pub mod client;
pub mod common;
pub mod error;
pub mod server;

// pub const TEST_PORT: u16 = 6665;
pub const COMMITTEE_SIZE: u32 = 16;
pub const NUM_CLIENTS: u32 = 64;
pub const KR_ID_LEN: usize = 128;
pub const PWD_LEN: usize = 16;
pub const GUESS_LIMIT: usize = 2;

pub fn init(params: SystemParams) -> (ServerState, ECPSSClientsPool) {
    let server_new_time = start_timer!(|| "init server state");
    let mut server_state = ServerState::new(params.clone());
    end_timer!(server_new_time);
    let client_new_time = start_timer!(|| "init client state");
    let mut clients_state = ECPSSClientsPool::new(params);
    end_timer!(client_new_time);

    // register clients with server
    let client_register_time = start_timer!(|| "register client");
    let clients_data = clients_state.register();
    end_timer!(client_register_time);
    println!("clients_data bytesize: {}", bincode::serialized_size(&clients_data).unwrap());
    println!("clients_data[0] bytesize: {}", bincode::serialized_size(&clients_data[0]).unwrap());
    let server_register_time = start_timer!(|| "register server");
    server_state.register_clients(clients_data);
    end_timer!(server_register_time);

    (server_state, clients_state)
}

pub fn committee_selection<'a, 'b>(
    mut server_state: ServerState<'a>,
    mut clients_state: ECPSSClientsPool<'b>,
) -> (ServerState<'a>, ECPSSClientsPool<'b>) {
    let root = server_state.get_root();
    let nomination_outputs = clients_state.sortition(&root);
    server_state.process_committee(&nomination_outputs);
    (server_state, clients_state)
}

pub fn distributed_keygen<'a>(
    mut server_state: ServerState<'a>,
    mut clients_state: ECPSSClientsPool<'a>,
) -> (ServerState<'a>, ECPSSClientsPool<'a>) {
    let epoch = server_state.epoch;
    assert!(
        epoch == 0,
        "Distributed Keygen can only be called at epoch 0"
    );
    assert!(
        epoch == clients_state.epoch,
        "Server and clients must be in sync"
    );

    // epoch 0
    // sample C0
    (server_state, clients_state) = committee_selection(server_state, clients_state);
    let committee_0 = server_state.get_committee(0);
    server_state.epoch += 1;
    clients_state.epoch += 1;
    println!("epoch 0 done");

    // epoch 1
    // sample C1
    (server_state, clients_state) = committee_selection(server_state, clients_state);
    let committee_1 = server_state.get_committee(1);
    // C0 reshares to C1
    let handovers = clients_state.contribute_dkg_randomness(
        &committee_0,
        &committee_1,
    );
    println!("num handovers: {}", handovers.len());
    server_state.process_state(&handovers);
    let commstate_1 = server_state.get_state(1);
    server_state.epoch += 1;
    clients_state.epoch += 1;
    println!("epoch 1 done");

    // epoch 2
    // sample C2
    (server_state, clients_state) = committee_selection(server_state, clients_state);
    let committee_2 = server_state.get_committee(2);
    let public_state = server_state.get_public_state();
    let handovers_and_rsps = clients_state.handover(
        &public_state,
        &committee_0,
        &committee_1,
        &committee_2,
        None,
        &commstate_1,
        None,
    );
    let handovers = handovers_and_rsps.iter().map(|(h, _)| h.clone()).collect();
    server_state.process_state(&handovers);
    server_state.epoch += 1;
    clients_state.epoch += 1;
    println!("epoch 2 done");

    (server_state, clients_state)
}

pub fn epoch<'a, 'b>(
    reqs: &Vec<RecoveryRequest>,
    mut server_state: ServerState<'a>,
    mut clients_state: ECPSSClientsPool<'b>,
) -> (Vec<RecoveryResponse>, ServerState<'a>, ECPSSClientsPool<'b>) {
    assert!(server_state.epoch == clients_state.epoch, "Server and clients must be in sync");
    // process reqs if any
    let reqs_batch = if reqs.is_empty() {
        None
    } else {
        Some(server_state.process_recovery_requests(reqs))
    };
    let reqs_batch = reqs_batch.as_ref();

    (server_state, clients_state) = committee_selection(server_state, clients_state);
    let public_state = server_state.get_public_state();
    let prev_committee = server_state.get_committee(server_state.epoch - 2);
    let curr_committee = server_state.get_committee(server_state.epoch - 1);
    let next_committee = server_state.get_committee(server_state.epoch);
    let prev_state = server_state.get_state(server_state.epoch - 2).pop().unwrap();
    let curr_state = server_state.get_state(server_state.epoch - 1);
    let handovers_and_rsps = clients_state.handover(
        &public_state,
        &prev_committee,
        &curr_committee,
        &next_committee,
        Some(&prev_state),
        &curr_state,
        reqs_batch,
    );
    let handovers = handovers_and_rsps.iter().map(|(h, _)| h.clone()).collect();
    server_state.process_state(&handovers);

    // process resps if any
    let recovery_responses = if reqs.is_empty() {
        Vec::new()
    } else {
        let rsps: Vec<RecoveryResponseBatch> = handovers_and_rsps.iter().filter_map(|(_, r)| match r {
            Some(r) => Some(r.clone()),
            None => None,
        }).collect();
        #[cfg(feature = "print-trace")]
        let start = start_timer!(|| "process recovery responses");
        let recovery_responses = server_state.process_recovery_responses(&rsps, reqs_batch.unwrap());
        #[cfg(feature = "print-trace")]
        end_timer!(start);
        recovery_responses
    };
    server_state.epoch += 1;
    clients_state.epoch += 1;

    (recovery_responses, server_state, clients_state)
}

pub mod tests {
    use std::collections::HashMap;

    use crate::crypto::ibe::Blind;
    use crate::crypto::ibe::{constraints::RecoveryRequestNIZK, BonehFranklinIBE};

    use self::common::{Fr, G1Affine, E, CommitteeData, CommitteeStateClient};
    use ark_crypto_primitives::snark::SNARK;
    use ark_ec::{bls12::Bls12Config, AffineRepr, CurveGroup};
    use ark_ff::{BigInteger, MontConfig, PrimeField, UniformRand};
    use ark_groth16::{Groth16, Proof};
    use ark_std::cfg_iter;
    use class_group::{CL_HSM, Integer};
    use client::{ECPSSClient, SecretRecoveryClient};
    use common::RecoveryRequest;
    use rand::seq::IteratorRandom;
    use rug::integer::Order;
    use rand::{thread_rng, Rng};
    use ark_bls12_377::Config as P;
    use ark_bls12_377::Bls12_377 as E1;
    use ark_bw6_761::BW6_761 as E2;

    use super::*;

    #[cfg(feature = "parallel")]
    use rayon::prelude::*;

    #[test]
    fn test_epoch() {
        let num_epochs = 2;
        let num_backups = 100;
        let num_recoveries_per_batch = 5;
        let mut rng = thread_rng();
        let q = Integer::from_digits(
            &ark_bls12_377::fr::FrConfig::MODULUS.to_bytes_le(),
            Order::Lsf,
        );
        let seed = Integer::from_str_radix("42", 10).expect("Integer: from_str_radix failed");
        let seclevel = 128;
        let g1_gen = G1Affine::generator();
        let g1_genprime = g1_gen
            .mul_bigint(Fr::rand(&mut ark_std::test_rng()).into_bigint())
            .into_affine();
        let nizk_setup = RecoveryRequestNIZK::<P> {
            request: None,
            client_id: None,
            pwd: None,
            blind: None,
            id_len: KR_ID_LEN,
            pwd_len: PWD_LEN,
        };
        let (groth_pk, groth_vk) =
            <Groth16<E2> as SNARK<<P as Bls12Config>::Fp>>::circuit_specific_setup(nizk_setup, &mut rng).unwrap();
        let params = SystemParams {
            num_clients: NUM_CLIENTS as usize,
            committee_size: COMMITTEE_SIZE as usize,
            threshold: (COMMITTEE_SIZE / 2) as usize,
            cl_params: CL_HSM::new(&q, &seed, seclevel),
            id_len: KR_ID_LEN,
            pwd_len: PWD_LEN,
            guess_limit: GUESS_LIMIT,
            g1_gen,
            g1_genprime,
            groth_pk: &groth_pk,
            groth_vk
        };
        let (mut server_state, mut clients_state) = init(params.clone());
        println!("Init done");
        /*
        for epoch_idx in 0..num_epochs {
            (server_state, clients_state) = epoch(server_state, clients_state);
            println!("Epoch: {}; {:?}, {:?}", server_state.epoch, server_state.committees[epoch_idx], server_state.get_shards());
        }
        */
        // DKG
        (server_state, clients_state) = distributed_keygen(server_state, clients_state);
        let ibe_pk = server_state.get_master_pk();
        // let committee_1 = server_state.get_committee(1);
        // let commstate_2 = server_state.get_state(2);
        // let mpk_client = clients_state.get_master_pk(&committee_1, &commstate_2[0]).unwrap();
        // assert_eq!(ibe_pk, mpk_client);
        println!("DKG done");

        // create backups
        let sr_clients_and_pwd_and_keys = (0..num_backups).into_iter().map(|i| {
            let sr_client_id = i;
            let pwd: [u8; PWD_LEN] = rng.gen();
            let sr_client = SecretRecoveryClient::new(sr_client_id, &ibe_pk, params.clone());
            let backup_secret: [u8; 32] = rng.gen();
            let backup_ciphertext = sr_client.backup(&pwd.to_vec(), &backup_secret.to_vec()).expect("backup failed");
            server_state.store_backup(sr_client_id, &backup_ciphertext);
            (sr_client, pwd, backup_secret)
        }).collect::<Vec<_>>();
        println!("Backups done");

        for _ in 0..num_epochs {
            println!("Epoch {}", server_state.epoch);
            // create recovery requests
            let recovery_ids: Vec<_> = (0..num_backups).choose_multiple(&mut rng, num_recoveries_per_batch);
            println!("recovery_ids: {:?}", recovery_ids);
            let reqs_and_blinds: Vec<(RecoveryRequest, Blind)> = cfg_iter!(recovery_ids).map(|&i| {
                let (sr_client, pwd, _) = &sr_clients_and_pwd_and_keys[i];
                sr_client.recovery_request(&pwd.to_vec()).expect("request recovery failed")
            }).collect();
            let mut reqs_and_blinds_hashmap = HashMap::new();
            reqs_and_blinds.iter().for_each(|(req, blind)| {
                reqs_and_blinds_hashmap.insert(req.client_id, (req.clone(), blind));
            });
            let reqs = reqs_and_blinds.iter().map(|(r, _)| r.clone()).collect();
            let resps: Vec<RecoveryResponse>;
            println!("Recovery Requests done");
            // process epoch
            (resps, server_state, clients_state) = epoch(&reqs, server_state, clients_state);
            println!("Recovery responses done");
            // recover keys and check
            resps.iter().for_each(|resp| {
                let (req, blind) = reqs_and_blinds_hashmap.get(&resp.client_id).unwrap();
                assert_eq!(resp.client_id, req.client_id);
                let (sr_client, pwd, backed_up_secret) = &sr_clients_and_pwd_and_keys[resp.client_id];
                let recovered_secret = sr_client.recover(&resp, &blind, &pwd.to_vec()).expect("recover failed");
                assert_eq!(backed_up_secret.to_vec(), recovered_secret);
            });
            println!("Keys recovered and matching");
        }
    }

    /*
    fn test_serialization() {
        let mut rng = thread_rng();
        let q = Integer::from_digits(
            &ark_bls12_377::fr::FrConfig::MODULUS.to_bytes_le(),
            Order::Lsf,
        );
        let seed = Integer::from_str_radix("42", 10).expect("Integer: from_str_radix failed");
        let seclevel = 128;
        let g1_gen = G1::generator();
        let g1_genprime = g1_gen
            .mul_bigint(Fr::rand(&mut ark_std::test_rng()).into_bigint())
            .into_affine();
        let num_epochs = 2;

        let params = SystemParams {
            num_clients: 20 as usize,
            committee_size: 10 as usize,
            threshold: 5 as usize,
            cl_params: CL_HSM::new(&q, &seed, seclevel),
            g1_gen,
            g1_genprime,
            id_len: KR_ID_LEN,
            pwd_len: PWD_LEN,
            groth_vk: todo!(),
            groth_pk: todo!(),
        };
        let (mut server_state, mut clients_state) = init(params);
        
        (server_state, clients_state) = distributed_keygen(server_state, clients_state);
        for _ in 0..num_epochs {
            (server_state, clients_state) = epoch(server_state, clients_state);
        }

        let committee_1 = server_state.get_committee(1);
        let committee_1_bytes = committee_1.to_bytes();
        let committee_1_ = CommitteeData::from_bytes(&committee_1_bytes);

        assert_eq!(committee_1.epoch, committee_1_.epoch);
        assert_eq!(committee_1.root, committee_1_.root);
        assert_eq!(committee_1.merkle_proof, committee_1_.merkle_proof);
        // assert_eq!(committee_1.members, committee_1_from_bytes.members); // should fail bc pke_proof not serialized

        let mut buffer = Vec::new();
        let commstate_1 = &server_state.get_state(1);
        buffer.extend(commstate_1.len().to_le_bytes());
        for state in commstate_1.iter() {
            buffer.extend(state.to_bytes());
        }
        let mut offset = 0;
        let len = usize::from_le_bytes(buffer[0..8].try_into().unwrap());
        offset += 8;
        let mut commstate_1_ = Vec::with_capacity(len);
        for i in 0..len {
            let state = CommitteeStateClient::from_bytes(&buffer[offset..]);
            offset += state.bytesize();
            commstate_1_.push(state);
        }
        assert_eq!(commstate_1_, *commstate_1);
    }
    */
}
