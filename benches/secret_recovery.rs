#[macro_use]
extern crate criterion;

use ark_ec::pairing::Pairing;
use ark_ec::{AffineRepr, CurveGroup};
use ark_ff::{BigInteger, MontConfig, PrimeField, UniformRand};
use ark_std::{cfg_into_iter, start_timer, end_timer};
use chorus::cfg_into_iter_client;
use chorus::crypto::avd::SingleStepAVD;
use chorus::crypto::ibe::{Blind, PublicKey as IBEPublicKey};
use chorus::network::{download_from_network, upload_to_network, ClientDKGUpload, ClientDownloadContribute, ClientDownloadDKG, ClientDownloadHandover, ClientHandoverUpload, NetworkRequest, API};
use class_group::{CL_HSM, Integer};
use criterion::Criterion;
use rand::thread_rng;
use rug::integer::Order;
use chorus::{read_from_file, write_to_file};
use chorus::secret_recovery::client::{ECPSSClientsPool, ECPSSClient, SecretRecoveryClient};
use chorus::secret_recovery::common::{CoefficientCommitments, Fr, G1Affine, GuessAttemptsMerkleTreeAVD, RecoveryRequest, RecoveryRequestBatch, RecoveryResponse, RecoveryResponseBatch, SortitionProof};
use chorus::secret_recovery::{
    KR_ID_LEN,
    PWD_LEN,
    GUESS_LIMIT,
    common::{
        CommitteeData, CommitteeStateClient,
        Handover, PublicState, SystemParams, E,
    },
    server::ServerState,
};
use std::collections::{HashMap, HashSet};
use std::env;
use ark_crypto_primitives::snark::SNARK;
use ark_ec::bls12::Bls12Config;
use ark_groth16::{Groth16, ProvingKey, VerifyingKey};
use ark_serialize::{CanonicalSerialize, CanonicalDeserialize};
use chorus::crypto::ibe::constraints::RecoveryRequestNIZK;
use std::fs::{self, File};
use std::io::prelude::*;
use std::path::Path;
use ark_bls12_377::Config as P;
use ark_bw6_761::BW6_761 as E2;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

#[derive(PartialEq)]
enum BenchmarkType {
    SaveState,
    Server,
    Client,
}

impl BenchmarkType {
    fn from_str(input: &str) -> Option<Self> {
        match input {
            "SAVE_STATE" => Some(BenchmarkType::SaveState),
            "SERVER" => Some(BenchmarkType::Server),
            "CLIENT" => Some(BenchmarkType::Client),
            _ => None,
        }
    }
}

const BENCH_CASES: [(usize, (usize, usize), usize, usize); 2] = [
    (1, (10, 50), 300, 1090),
    (2, (1, 75), 121, 1214),
];

const NUM_CLIENTS: [usize; 3] = [
    10usize.pow(6),
    10usize.pow(7),
    10usize.pow(8),
];

const NETWORK_IP_FOR_CLIENTS: &str = "0.0.0.0";
const NETWORK_IP_FOR_SERVER: &str = "0.0.0.0";

fn unique_elements<T: std::hash::Hash + Eq + Clone>(vec: Vec<T>) -> Vec<T> {
    let set: HashSet<_> = vec.into_iter().collect();
    set.into_iter().collect()
}

fn human_readable_format(num: usize) -> String {
    if num >= 1_000_000_000 {
        format!("{:.0}B", num as f64 / 1_000_000_000.0)
    } else if num >= 1_000_000 {
        format!("{:.0}M", num as f64 / 1_000_000.0)
    } else if num >= 1_000 {
        format!("{:.0}K", num as f64 / 1_000.0)
    } else {
        num.to_string()
    }
}

fn truncate_committee(committee: &CommitteeData, truncated_size: usize) -> CommitteeData {
    let mut truncated_committee = committee.clone();
    truncated_committee.members = truncated_committee.members[..truncated_size].to_vec();
    truncated_committee.merkle_proof = truncated_committee.merkle_proof[..truncated_size].to_vec();
    truncated_committee
}

fn custom_config() -> Criterion {
    Criterion::default().sample_size(10) // Reduces the number of samples
}

fn bench_client_new(c: &mut Criterion, params: &SystemParams) {
    c.bench_function(&format!("client-new"), move |b| {
        b.iter(|| {
            let _ = ECPSSClient::new(0, params.clone());
        })
    });
}

fn bench_sortition(c: &mut Criterion,
                   ecpss_client: &mut ECPSSClient,
                   root: &Vec<u8>,
                   params: &SystemParams,
                   epoch: usize) {
    c.bench_function(&format!("sortition"), move |b| {
        b.iter(|| {
            ecpss_client.sortition(root, params, epoch).unwrap();
        })
    });
}

fn bench_dkg_contribute(c: &mut Criterion,
                        ecpss_client: &mut ECPSSClient,
                        curr_committee: &CommitteeData,
                        next_committee: &CommitteeData,
                        params: &SystemParams,
                        epoch: usize) {
    c.bench_function(&format!("dkg-contribute"), move |b| {
        b.iter(|| {
            let seat = ecpss_client.on_committee(curr_committee, params, epoch-1).unwrap().unwrap();
            ecpss_client.contribute_dkg_randomness(&seat, next_committee, params, epoch).unwrap();
        })
    });
}

fn bench_dkg_contribute_with_network(c: &mut Criterion,
                        ecpss_client: &mut ECPSSClient,
                        params: &SystemParams,
                        epoch: usize,
                        case: usize) {
    c.bench_function(&format!("dkg-contribute-with-network"), move |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| {
            rt.block_on(async {
                let request = NetworkRequest {
                    case,
                    num_clients: params.num_clients,
                    api: API::DKGContribute,
                };
                #[cfg(feature = "print-trace")]
                let download_time = start_timer!(|| "download for dkg-contribute");
                let (download, mut stream) = download_from_network::<ClientDownloadContribute>(&NETWORK_IP_FOR_CLIENTS, &request).await.unwrap();
                #[cfg(feature = "print-trace")]
                end_timer!(download_time);
                let seat = ecpss_client.on_committee(&download.committee_0, params, epoch-1).unwrap().unwrap();
                let handover = ecpss_client.contribute_dkg_randomness(&seat, &download.committee_1, params, epoch).unwrap().unwrap();
                #[cfg(feature = "print-trace")]
                let upload_time = start_timer!(|| "upload for dkg-contribute");
                upload_to_network::<Handover>(&mut stream, &handover).await.unwrap();
                #[cfg(feature = "print-trace")]
                end_timer!(upload_time);
            });
        })
    });
}

fn bench_handover_dkg(c: &mut Criterion,
                            ecpss_client: &mut ECPSSClient,
                            new_public_state: &PublicState,
                            prev_committee: &CommitteeData,
                            curr_committee: &CommitteeData,
                            next_committee: &CommitteeData,
                            prev_state: Option<&CoefficientCommitments>,
                            curr_state: &CommitteeStateClient,
                            reqs: Option<&RecoveryRequestBatch>,
                            params: &SystemParams,
                            epoch: usize) {
    c.bench_function(&format!("handover-dkg"), move |b| {
        b.iter(|| {
            let seat = ecpss_client.on_committee(curr_committee, params, epoch-1).unwrap().unwrap();
            ecpss_client.handover(
                    &seat,
                    new_public_state,
                    prev_committee,
                    next_committee,
                    prev_state,
                    &curr_state,
                    reqs,
                    params,
                    epoch,
                )
                .unwrap();
        })
    });
}

fn bench_handover_dkg_with_network(c: &mut Criterion,
                            ecpss_client: &mut ECPSSClient,
                            params: &SystemParams,
                            epoch: usize,
                            case: usize) {
    c.bench_function(&format!("handover-dkg-with-network"), move |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| {
            rt.block_on(async {
                let request = NetworkRequest {
                    case,
                    num_clients: params.num_clients,
                    api: API::DKGHandover,
                };
                #[cfg(feature = "print-trace")]
                let download_time = start_timer!(|| "download for handover-dkg");
                let (download, mut stream) = download_from_network::<ClientDownloadDKG>(&NETWORK_IP_FOR_CLIENTS, &request).await.unwrap();
                #[cfg(feature = "print-trace")]
                end_timer!(download_time);
                let seat = ecpss_client.on_committee(&download.committee_1, params, epoch-1).unwrap().unwrap();
                let (handover, _) = ecpss_client.handover(&seat, &download.public_state_epoch_2, &download.committee_0, &download.committee_2, None, &download.commstate_1, None, params, epoch).unwrap().unwrap();
                #[cfg(feature = "print-trace")]
                let upload_time = start_timer!(|| "upload for handover-dkg");
                upload_to_network::<Handover>(&mut stream, &handover).await.unwrap();
                #[cfg(feature = "print-trace")]
                end_timer!(upload_time);
            });
        })
    });
}

fn bench_handover_typical(c: &mut Criterion,
                            ecpss_client: &mut ECPSSClient,
                            new_public_state: &PublicState,
                            prev_committee: &CommitteeData,
                            curr_committee: &CommitteeData,
                            next_committee: &CommitteeData,
                            prev_state: Option<&CoefficientCommitments>,
                            curr_state: &CommitteeStateClient,
                            reqs: Option<&RecoveryRequestBatch>,
                            params: &SystemParams,
                            epoch: usize) {
    c.bench_function(&format!("handover-typical"), move |b| {
        b.iter(|| {
            let seat = ecpss_client.on_committee(curr_committee, params, epoch-1).unwrap().unwrap();
            ecpss_client.handover(
                    &seat,
                    new_public_state,
                    prev_committee,
                    next_committee,
                    prev_state,
                    &curr_state,
                    reqs,
                    params,
                    epoch,
                )
                .unwrap();
        })
    });
}

fn bench_handover_typical_with_network(c: &mut Criterion,
                            ecpss_client: &mut ECPSSClient,
                            params: &SystemParams,
                            epoch: usize,
                            case: usize) {
    c.bench_function(&format!("handover-typical-with-network"), move |b| {
        let rt = tokio::runtime::Runtime::new().unwrap();
        b.iter(|| {
            rt.block_on(async {
                let request = NetworkRequest {
                    case,
                    num_clients: params.num_clients,
                    api: API::TypicalHandover,
                };
                #[cfg(feature = "print-trace")]
                let download_time = start_timer!(|| "download for handover-typical");
                let (download, mut stream) = download_from_network::<ClientDownloadHandover>(&NETWORK_IP_FOR_CLIENTS, &request).await.unwrap();
                #[cfg(feature = "print-trace")]
                end_timer!(download_time);
                let seat = ecpss_client.on_committee(&download.committee_2, params, epoch-1).unwrap().unwrap();
                let handover_and_rsps_batch = ecpss_client.handover(&seat, &download.public_state_epoch_3, &download.committee_1, &download.committee_3, Some(&download.commstate_1_coeff_cmts), &download.commstate_2, Some(&download.reqs_batch), params, epoch).unwrap().unwrap();
                #[cfg(feature = "print-trace")]
                let upload_time = start_timer!(|| "upload for handover-typical");
                upload_to_network::<(Handover, Option<RecoveryResponseBatch>)>(&mut stream, &handover_and_rsps_batch).await.unwrap();
                #[cfg(feature = "print-trace")]
                end_timer!(upload_time);
            });
        })
    });
}

fn bench_process_committee(c: &mut Criterion, server: &mut ServerState, nominations: &Vec<SortitionProof>) {
    c.bench_function(&format!("process-committee"), move |b| {
        b.iter(|| {
            server.process_committee(nominations);
            // resetting the state
            server.committees.pop();
        })
    });
}

fn bench_process_state(c: &mut Criterion, server: &mut ServerState, handovers: &Vec<Handover>) {
    c.bench_function(&format!("process-state"), move |b| {
        b.iter(|| {
            server.process_state(handovers);
            // resetting the state
            server.states.pop();
            if server.epoch > 1 {
                // if epoch > 1, we also pop the pushed share_cmts
                server.share_cmts.pop();
            }
        })
    });
}

fn bench_backup(c: &mut Criterion, kr_client: &SecretRecoveryClient, pwd: &Vec<u8>, key_to_backup: &Vec<u8>) {
    c.bench_function(&format!("backup"), move |b| {
        b.iter(|| {
            kr_client.backup(pwd, key_to_backup).unwrap();
        })
    });
}

fn bench_recovery_request(c: &mut Criterion, kr_client: &SecretRecoveryClient, pwd: &Vec<u8>) {
    c.bench_function(&format!("recovery-request"), move |b| {
        b.iter(|| {
            kr_client.recovery_request(pwd).unwrap();
        })
    });
}

fn bench_recover(c: &mut Criterion, kr_client: &SecretRecoveryClient, rsp: &RecoveryResponse, blind: &Blind, pwd: &Vec<u8>) {
    c.bench_function(&format!("recover"), move |b| {
        b.iter(|| {
            kr_client.recover(rsp, blind, pwd).unwrap();
        })
    });
}

fn bench_server_process_recovery_requests(c: &mut Criterion, server: &mut ServerState, reqs: &Vec<RecoveryRequest>) {
    c.bench_function(&format!("server-process-recovery-requests"), move |b| {
        b.iter(|| {
            server.process_recovery_requests(reqs);
            // resetting the state
            server.guess_attempts_mt = GuessAttemptsMerkleTreeAVD::new(&mut thread_rng(), &()).unwrap();
        })
    });
}

fn bench_server_process_recovery_responses(c: &mut Criterion, server: &mut ServerState, rsps: &Vec<RecoveryResponseBatch>, reqs: &RecoveryRequestBatch) {
    c.bench_function(&format!("server-process-recovery-responses"), move |b| {
        b.iter(|| {
            server.process_recovery_responses(rsps, reqs);
        })
    });
}

#[cfg(feature = "deterministic")]
fn benches_class(c: &mut Criterion) {
    use core::num;
    use std::io::BufReader;

    use chorus::{secret_recovery::common::{BackupCiphertext, ECPSSClientData, RecoveryResponseBatch}, serialized_size};
    use rand::{rngs::StdRng, thread_rng, Rng, SeedableRng};
    use rand_chacha::ChaCha20Rng;


    let benchmark_type_str = env::var("BENCHMARK_TYPE").expect("BENCHMARK_TYPE not found");
    let benchmark_type = BenchmarkType::from_str(&benchmark_type_str).unwrap();

    // set DETERMINISTIC_TEST_RNG = 1 so that ark_std::test_rng() is deterministic
    std::env::set_var("DETERMINISTIC_TEST_RNG", "1");
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
    let (groth_pk, groth_vk) = match benchmark_type {
        // BenchmarkType::SaveState | BenchmarkType::Server => {
        BenchmarkType::SaveState | BenchmarkType::Server | BenchmarkType::Client => {
            let nizk_setup = RecoveryRequestNIZK::<P> {
                request: None,
                client_id: None,
                pwd: None,
                blind: None,
                id_len: KR_ID_LEN,
                pwd_len: PWD_LEN,
            };
            #[cfg(feature = "deterministic")]
            let mut csprng = ChaCha20Rng::seed_from_u64(0u64);
            #[cfg(not(feature = "deterministic"))]
            let mut csprng = OsRng::default();
            let (groth_pk, groth_vk) = <Groth16<E2> as SNARK<<P as Bls12Config>::Fp>>::circuit_specific_setup(nizk_setup, &mut csprng).unwrap();
            if benchmark_type == BenchmarkType::SaveState {
                let groth_pk_path = Path::new("groth_pk");
                let mut groth_pk_file = File::create(groth_pk_path).unwrap();
                let mut bytes = Vec::new();
                groth_pk.serialize_compressed(&mut bytes).expect("groth_pk serialization failed");
                groth_pk_file.write_all(&bytes).expect("error writing groth_pk file");
                println!("wrote groth_pk");

                let groth_vk_path = Path::new("groth_vk");
                let mut groth_vk_file = File::create(groth_vk_path).unwrap();
                let mut bytes = Vec::new();
                groth_vk.serialize_compressed(&mut bytes).expect("groth_vk serialization failed");
                groth_vk_file.write_all(&bytes).expect("error writing groth_vk file");
                println!("wrote groth_vk");
            }
            (groth_pk, groth_vk)
        },
    };

    for num_clients in NUM_CLIENTS.iter() {
        let num_clients = *num_clients;
        let params = SystemParams {
            num_clients,
            cl_params: CL_HSM::new(&q, &seed, seclevel),
            g1_gen,
            g1_genprime,
            committee_size: num_clients, // to make benchmarking easy, we want all clients to be on the committee
            threshold: 0, // setting dummy threshold value for now
            id_len: KR_ID_LEN,
            pwd_len: PWD_LEN,
            guess_limit: GUESS_LIMIT,
            groth_pk: &groth_pk,
            groth_vk: groth_vk.clone()
        };
        // we will randomly select some 3000 clients to be on the committee and the rest will just be clones of them
        let mut rng = ChaCha20Rng::seed_from_u64(0);
        let mut on_committee_idx = [0; 3000];
            for i in 0..on_committee_idx.len() {
                on_committee_idx[i] = rng.gen_range(0..num_clients);
            }
        let on_committee_idx = unique_elements(on_committee_idx.to_vec());
        println!("on_committee_idx len: {:?}", on_committee_idx.len());
        println!("num_clients: {:?}", num_clients);

        let mut server_state_static: Option<ServerState> = match benchmark_type {
            BenchmarkType::SaveState | BenchmarkType::Server => {
                let on_committee_clients = cfg_into_iter!(on_committee_idx.clone()).map(|i| {
                    ECPSSClient::new(i, params.clone())
                }).collect::<Vec<_>>();
                let dummy_client = ECPSSClient::new(0, params.clone());
                let dummy_client_data = dummy_client.register();
                let mut client_data_with_dummies = vec![dummy_client_data; num_clients];
                (0..num_clients).into_iter().for_each(|i| {
                    client_data_with_dummies[i].id = i;
                });
                on_committee_clients.iter().for_each(|c| {
                    let client_data = c.register();
                    client_data_with_dummies[c.id] = client_data;
                }); 
                let mut server_state = ServerState::new(params.clone());
                server_state.register_clients(client_data_with_dummies);
                Some(server_state)
            },
            BenchmarkType::Client => {
                let start = chorus::start_stat_tracking!();
                bench_client_new(c, &params);
                chorus::end_stat_tracking!(start);
                None
            }
        };
    for (case, (corr, fail), t, n) in BENCH_CASES.iter() {
        let (case, corr, fail, t, n) = (*case, *corr, *fail, *t, *n);
        let threshold = t as usize;
        let committee_size = n as usize;

        println!("case: {}, corrupt fraction: {}, fail fraction: {}, threshold: {}, committee_size: {}, num_clients: {}", case, corr, fail, threshold, committee_size, num_clients);

        let params = SystemParams {
            num_clients,
            committee_size: num_clients, // to make benchmarking easy, we want all clients to be on the committee
            threshold,
            cl_params: CL_HSM::new(&q, &seed, seclevel),
            g1_gen,
            g1_genprime,
            id_len: KR_ID_LEN,
            pwd_len: PWD_LEN,
            guess_limit: GUESS_LIMIT,
            groth_pk: &groth_pk,
            groth_vk: groth_vk.clone()
        };

        let dir_name = format!("case_{}_clients_{}", case, human_readable_format(num_clients));

        let expected_committee_size = committee_size; // worst case
        let max_handovers_needed = 2 * threshold; // 2 * threshold because at most threshold - 1 clients can be corrupted and send malformed handovers
        println!("expected_committee_size: {}, max_handovers_needed: {}", expected_committee_size, expected_online_committee_members, max_handovers_needed);
        let (mut clients_state, mut server_state) = match benchmark_type {
            BenchmarkType::SaveState | BenchmarkType::Server => {
                // create folder for info about this case
                if benchmark_type == BenchmarkType::SaveState {
                    fs::create_dir_all(&dir_name).expect("Failed to create directory");
                }

                let mut server_state = server_state_static.unwrap();
                server_state.params = params.clone();
                // reset state for this case
                server_state.reset_state_except_clients();

                // we only want committee_size clients to be on the committee
                let on_committee_clients_state = cfg_into_iter!(on_committee_idx[..expected_committee_size]).map(|i| {
                    ECPSSClient::new(*i, params.clone())
                }).collect::<Vec<_>>();
                let clients_state = ECPSSClientsPool { params: params.clone(), states: on_committee_clients_state, epoch: 0 };

                (Some(clients_state), Some(server_state))
            },
            BenchmarkType::Client => {
                (None, None)
            }
        };

        let root = match benchmark_type {
            BenchmarkType::SaveState | BenchmarkType::Server => {
                let root = server_state.as_ref().unwrap().get_root(); 
                if benchmark_type == BenchmarkType::SaveState {
                    // store root in file
                    let root_path = Path::new(&dir_name).join("root");
                    let mut root_file = File::create(root_path).unwrap();
                    root_file.write_all(&root).expect("error writing root file");
                    println!("root (written): {:?}", root);
                }
                root
            },
            BenchmarkType::Client => {
                let root_path = Path::new(&dir_name).join("root");
                let mut root_file = File::open(root_path).expect("error opening root file");
                let mut root = Vec::new();
                root_file.read_to_end(&mut root).expect("error reading root file");
                println!("root (read): {:?}", root);
                root
            }
        };

        // Epoch 0
        // Bench Sortition
        match benchmark_type {
            BenchmarkType::SaveState | BenchmarkType::Server => {
                let nominations = clients_state.as_mut().unwrap().sortition(&root);
                let max_nomination_size = nominations
                    .iter()
                    .map(|c| serialized_size!(c))
                    .max()
                    .unwrap();
                println!("Total Nomination bytesize: {} bytes", nominations.iter().map(|c| serialized_size!(c)).sum::<usize>());
                println!("Max Nomination bytesize: {} bytes", max_nomination_size);
                println!("Nominations len: {:?}", nominations.len());
                
                server_state.as_mut().unwrap().process_committee(&nominations);

                let committee_0 = server_state.as_mut().unwrap().get_committee(0);
                println!("committee_0 bytesize: {} bytes", serialized_size!(committee_0));
                println!("committee_0 without Merkle proof bytesize: {} bytes", committee_0.bytesize_without_merkle_proof());
                // store committee 0
                if benchmark_type == BenchmarkType::SaveState {
                    write_to_file!(nominations[0].client_id, &dir_name, "nomination_id");
                    write_to_file!(committee_0, &dir_name, "committee_0");
                } else {
                    let start = chorus::start_stat_tracking!();
                    bench_process_committee(c, server_state.as_mut().unwrap(), &nominations);
                    chorus::end_stat_tracking!(start);
                }

                server_state.as_mut().unwrap().epoch += 1;
                clients_state.as_mut().unwrap().epoch += 1;
            },
            BenchmarkType::Client => {
                let nomination_id = read_from_file!(&dir_name, "nomination_id", usize);
                println!("nomination_id: {:?}", nomination_id);
                let mut nomination_client_state = ECPSSClient::new(nomination_id, params.clone());

                let start = chorus::start_stat_tracking!();
                println!("> Bench Sortition for a client on the committee");
                bench_sortition(c, &mut nomination_client_state, &root, &params, 0);
                chorus::end_stat_tracking!(start);
            }

        }

        // Epoch 1
        // DKG Contribute
        match benchmark_type {
            BenchmarkType::SaveState | BenchmarkType::Server => {
                let nominations = clients_state.as_mut().unwrap().sortition(&root);
                server_state.as_mut().unwrap().process_committee(&nominations);
                let committee_0 = server_state.as_mut().unwrap().get_committee(0);
                let committee_1 = server_state.as_mut().unwrap().get_committee(1);
                println!("committee_1 bytesize: {} bytes", serialized_size!(committee_1));
                println!("committee_1 without Merkle proof bytesize: {} bytes", committee_1.bytesize_without_merkle_proof());
            
                let committee_0_truncated = truncate_committee(&committee_0, max_handovers_needed);
                let handovers =
                    clients_state.as_mut().unwrap().contribute_dkg_randomness(&committee_0_truncated, &committee_1);
                let max_contribution_size = handovers
                    .iter()
                    .map(|c| serialized_size!(c))
                    .max()
                    .unwrap();
                println!("Total Contribution bytesize: {} bytes", handovers.iter().map(|c| serialized_size!(c)).sum::<usize>());
                println!("Max Contribution bytesize: {} bytes", max_contribution_size);
                println!("Contributions len: {:?}", handovers.len());

                if benchmark_type == BenchmarkType::SaveState {
                    write_to_file!(committee_1, &dir_name, "committee_1");
                    write_to_file!(handovers[0].client_id, &dir_name, "dkg_contribute_id");
                } else {
                    let start = chorus::start_stat_tracking!();
                    println!("> Bench Process State for DKG Contributions");
                    bench_process_state(c, server_state.as_mut().unwrap(), &handovers);
                    chorus::end_stat_tracking!(start);
                }

                server_state.as_mut().unwrap().process_state(&handovers);
                let commstate_1 = server_state.as_mut().unwrap().get_state(1);
                println!("Total commstate_1 bytesize: {} bytes", commstate_1.iter().map(|c| serialized_size!(c)).sum::<usize>());
                println!("Max commstate_1 bytesize: {} bytes", commstate_1.iter().map(|c| serialized_size!(c)).max().unwrap());

                server_state.as_mut().unwrap().epoch += 1;
                clients_state.as_mut().unwrap().epoch += 1;
            },
            BenchmarkType::Client => {
                let committee_0 = read_from_file!(&dir_name, "committee_0", CommitteeData);
                let committee_1 = read_from_file!(&dir_name, "committee_1", CommitteeData);
                let dkg_contribute_id = read_from_file!(&dir_name, "dkg_contribute_id", usize);
                println!("dkg_contribute_id: {:?}", dkg_contribute_id);
                let mut dkg_contribute_client_state = ECPSSClient::new(dkg_contribute_id, params.clone());

                let start = chorus::start_stat_tracking!();
                println!("> Bench DKG Contribute for a client that contributes");
                bench_dkg_contribute(c, &mut dkg_contribute_client_state, &committee_0, &committee_1, &params, 1);
                // bench_dkg_contribute_with_network(c, &mut dkg_contribute_client_state, &params, 1, case);
                chorus::end_stat_tracking!(start);
            }
        }

        // Epoch 2
        // Bench Handover
        match benchmark_type {
            BenchmarkType::SaveState | BenchmarkType::Server => {
                let nominations = clients_state.as_mut().unwrap().sortition(&root);
                server_state.as_mut().unwrap().process_committee(&nominations);
                let committee_0 = server_state.as_mut().unwrap().get_committee(0);
                let committee_1 = server_state.as_mut().unwrap().get_committee(1);
                let commstate_1 = server_state.as_mut().unwrap().get_state(1);
                let committee_2 = server_state.as_mut().unwrap().get_committee(2); 
                println!("committee_2 bytesize: {} bytes", serialized_size!(committee_2));
                println!("committee_2 without Merkle proof bytesize: {} bytes", committee_2.bytesize_without_merkle_proof());
                
                let public_state = server_state.as_mut().unwrap().get_public_state();
                println!("public_state_epoch_2 bytesize: {} bytes", public_state.bytesize());

                let committee_1_truncated = truncate_committee(&committee_1, max_handovers_needed);
                // Bench Handover
                let handovers_and_rsps = clients_state.as_mut().unwrap().handover(
                    &public_state,
                    &committee_0,
                    &committee_1_truncated,
                    &committee_2,
                    None,
                    &commstate_1,
                    None
                );
                let handovers: Vec<Handover> = handovers_and_rsps.iter().map(|(h, _)| h.clone()).collect();
                let max_handover_size = handovers
                    .iter()
                    .map(|c| serialized_size!(c))
                    .max()
                    .unwrap();
                println!("Total Handover (DKG) bytesize: {} bytes", handovers.iter().map(|c| serialized_size!(c)).sum::<usize>());
                println!("Max Handover (DKG) bytesize: {} bytes", max_handover_size);
                println!("Handovers (DKG) len: {:?}", handovers.len());

                if benchmark_type == BenchmarkType::SaveState {
                    // store committee 2
                    write_to_file!(committee_2, &dir_name, "committee_2");
                    // store public_state
                    write_to_file!(public_state, &dir_name, "public_state_epoch_2");
                    write_to_file!(handovers[0].client_id, &dir_name, "dkg_handover_id");
                    // store commstate_1 for seat_idx
                    write_to_file!(commstate_1[handovers[0].seat_idx - 1], &dir_name, "commstate_1_seat_idx");
                } else {
                    let start = chorus::start_stat_tracking!();
                    println!("> Bench Process State for DKG Handover");
                    bench_process_state(c, server_state.as_mut().unwrap(), &handovers);
                    chorus::end_stat_tracking!(start);
                }
                
                server_state.as_mut().unwrap().process_state(&handovers);
                let commstate_2 = server_state.as_mut().unwrap().get_state(2);
                println!("Total commstate_2 bytesize: {} bytes", commstate_2.iter().map(|c| serialized_size!(c)).sum::<usize>());
                println!("Max commstate_2 bytesize: {} bytes", commstate_2.iter().map(|c| serialized_size!(c)).max().unwrap());

                server_state.as_mut().unwrap().epoch += 1;
                clients_state.as_mut().unwrap().epoch += 1;
            },
            BenchmarkType::Client => {
                let committee_0 = read_from_file!(&dir_name, "committee_0", CommitteeData);
                let committee_1 = read_from_file!(&dir_name, "committee_1", CommitteeData);
                let committee_2 = read_from_file!(&dir_name, "committee_2", CommitteeData);
                let commstate_1 = read_from_file!(&dir_name, "commstate_1_seat_idx", CommitteeStateClient);
                let public_state_epoch_2 = read_from_file!(&dir_name, "public_state_epoch_2", PublicState);
                let dkg_handover_id = read_from_file!(&dir_name, "dkg_handover_id", usize);
                let mut dkg_handover_client_state = ECPSSClient::new(dkg_handover_id, params.clone());

                let start = chorus::start_stat_tracking!();
                println!("> Bench Handover (DKG) for a client that does handover");
                bench_handover_dkg(c, &mut dkg_handover_client_state, &public_state_epoch_2, &committee_0, &committee_1, &committee_2, None, &commstate_1, None, &params, 2);
                // bench_handover_dkg_with_network(c, &mut dkg_handover_client_state, &params, 2, case);
                chorus::end_stat_tracking!(start);
            },
        }

        // get ibe_pk
        let ibe_pk = match benchmark_type {
            BenchmarkType::SaveState | BenchmarkType::Server => {
                let ibe_pk = server_state.as_mut().unwrap().get_master_pk();
                ibe_pk
            },
            BenchmarkType::Client => {
                read_from_file!(&dir_name, "ibe_pk", IBEPublicKey)
            }
        };
        
        let epoch_duration = 2 as f64; // in minutes
        let expected_requests_per_year = num_clients as f64;
        let expected_requests_per_day = expected_requests_per_year / (365 as f64);
        let expected_requests_per_hour = expected_requests_per_day / (24 as f64);
        let expected_requests_per_epoch = epoch_duration * expected_requests_per_hour / (60 as f64);
        let expected_requests_per_epoch = expected_requests_per_epoch.ceil() as usize;
        println!("expected_requests_per_epoch: {:?}", expected_requests_per_epoch);
        // Epoch 3
        // Bench Handover
        match benchmark_type {
            BenchmarkType::SaveState | BenchmarkType::Server => {
                let nominations = clients_state.as_mut().unwrap().sortition(&root);
                server_state.as_mut().unwrap().process_committee(&nominations);
                let committee_1 = server_state.as_mut().unwrap().get_committee(1);
                let committee_2 = server_state.as_mut().unwrap().get_committee(2);
                let committee_3 = server_state.as_mut().unwrap().get_committee(3);
                let commstate_1 = server_state.as_mut().unwrap().get_state(1);
                let commstate_2 = server_state.as_mut().unwrap().get_state(2);
                println!("committee_3 bytesize: {} bytes", serialized_size!(committee_3));
                println!("committee_3 without Merkle proof bytesize: {} bytes", committee_3.bytesize_without_merkle_proof());

                let mut rng = StdRng::seed_from_u64(0);
                let sr_client_ids = on_committee_idx[..expected_requests_per_epoch].to_vec();
                let pwds: Vec<[u8; PWD_LEN]> = sr_client_ids.iter().map(|_| rng.gen()).collect();
                let backup_keys: Vec<[u8; 32]> = sr_client_ids.iter().map(|_| rng.gen()).collect();
                let sr_clients = sr_client_ids.iter().map(|i| SecretRecoveryClient::new(*i, &ibe_pk, params.clone())).collect::<Vec<_>>();
                let backup_ciphertexts = sr_clients.iter().zip(pwds.iter().zip(backup_keys.iter())).map(|(c, (p, k))| c.backup(&p.to_vec(), &k.to_vec()).unwrap()).collect::<Vec<_>>();
                println!("Backup Ciphertext bytesize: {} bytes", serialized_size!(backup_ciphertexts[0]));
                (0..sr_client_ids.len()).into_iter().for_each(|i| {
                    server_state.as_mut().unwrap().store_backup(sr_client_ids[i], &backup_ciphertexts[i]);
                });
                let (reqs, blinds): (Vec<RecoveryRequest>, Vec<Blind>) = cfg_into_iter!(0..sr_client_ids.len()).chunks(20).map(|chunk| {
                    chunk.iter().map(|i| {
                        let (req, blind) = sr_clients[*i].recovery_request(&pwds[*i].to_vec()).expect("request recovery failed");
                        (req, blind)
                    }).collect::<Vec<_>>()
                }).flatten().unzip();
                println!("Recovery Request bytesize: {} bytes", serialized_size!(reqs[0]));

                if benchmark_type == BenchmarkType::Server {
                    let start = chorus::start_stat_tracking!();
                    println!("> Bench Process Recovery Requests for server");
                    bench_server_process_recovery_requests(c, server_state.as_mut().unwrap(), &reqs);
                    chorus::end_stat_tracking!(start);
                }

                let reqs_batch = server_state.as_mut().unwrap().process_recovery_requests(&reqs);
                println!("Recovery Request Batch bytesize: {} bytes", serialized_size!(reqs_batch));

                // Bench Handover
                let prev_state = &commstate_1[0];
                println!("prev_state bytesize: {} bytes", serialized_size!(prev_state.coeff_cmts));
                let public_state = server_state.as_mut().unwrap().get_public_state();
                println!("public_state_epoch_3 bytesize: {} bytes", public_state.bytesize());

                let committee_2_truncated = truncate_committee(&committee_2, max_handovers_needed);
                let handovers_and_rsps = clients_state.as_mut().unwrap().handover(
                    &public_state,
                    &committee_1,
                    &committee_2_truncated,
                    &committee_3,
                    Some(prev_state),
                    &commstate_2,
                    Some(&reqs_batch),
                );
                let handovers: Vec<Handover> = handovers_and_rsps.iter().map(|(h, _)| h.clone()).collect();
                let max_handover_size = handovers
                    .iter()
                    .map(|c| serialized_size!(c))
                    .max()
                    .unwrap();
                println!("Total Handover (Typical) bytesize: {} bytes", handovers.iter().map(|c| serialized_size!(c)).sum::<usize>());
                println!("Max Handover (Typical) bytesize: {} bytes", max_handover_size);
                println!("Handovers (Typical) len: {:?}", handovers.len());

                let rsps: Vec<RecoveryResponseBatch> = handovers_and_rsps.iter().map(|(_, r)| r.as_ref().unwrap().clone()).collect();
                let max_rsp_size = rsps
                    .iter()
                    .map(|c| serialized_size!(c))
                    .max()
                    .unwrap();
                println!("Total Recovery Response Batch bytesize: {} bytes", rsps.iter().map(|c| serialized_size!(c)).sum::<usize>());
                println!("Max Recovery Response Batch bytesize: {} bytes", max_rsp_size);
                println!("Recovery Responses Batch len: {:?}", rsps.len());

                server_state.as_mut().unwrap().process_state(&handovers);
                let commstate_3 = server_state.as_mut().unwrap().get_state(3);
                println!("Total commstate_3 bytesize: {} bytes", commstate_3.iter().map(|c| serialized_size!(c)).sum::<usize>());
                println!("Max commstate_3 bytesize: {} bytes", commstate_3.iter().map(|c| serialized_size!(c)).max().unwrap());

                let recovery_responses = server_state.as_ref().unwrap().process_recovery_responses(&rsps, &reqs_batch);
                println!("Total Recovery Responses bytesize: {} bytes", recovery_responses.iter().map(|c| serialized_size!(c)).sum::<usize>());
                println!("Max Recovery Responses bytesize: {} bytes", recovery_responses.iter().map(|c| serialized_size!(c)).max().unwrap());
                println!("Recovery Responses len: {:?}", recovery_responses.len());

                if benchmark_type == BenchmarkType::SaveState {
                    // store committee 3
                    write_to_file!(committee_3, &dir_name, "committee_3");
                    // store public_state
                    write_to_file!(public_state, &dir_name, "public_state_epoch_3");
                    write_to_file!(handovers[0].client_id, &dir_name, "typical_handover_id");
                    // store commstate_2 for seat_idx
                    write_to_file!(commstate_2[handovers[0].seat_idx - 1], &dir_name, "commstate_2_seat_idx");
                    // ibe_pk
                    write_to_file!(ibe_pk, &dir_name, "ibe_pk");
                    // pwd
                    write_to_file!(pwds[0], &dir_name, "pwd");
                    // blind
                    write_to_file!(blinds[0], &dir_name, "blind");
                    // backup_key
                    write_to_file!(backup_keys[0], &dir_name, "backup_key");
                    // backup_ciphertext
                    write_to_file!(backup_ciphertexts[0], &dir_name, "backup_ciphertext");
                    // reqs_batch
                    write_to_file!(reqs_batch, &dir_name, "requests_batch");
                    // recovery_responses
                    write_to_file!(recovery_responses[0], &dir_name, "recovery_response");
                } else {
                    let start = chorus::start_stat_tracking!();
                    println!("> Bench Process State for Typical Handover");
                    bench_process_state(c, server_state.as_mut().unwrap(), &handovers);
                    chorus::end_stat_tracking!(start);

                    let start = chorus::start_stat_tracking!();
                    println!("> Bench Process Recovery Responses for server");
                    bench_server_process_recovery_responses(c, server_state.as_mut().unwrap(), &rsps, &reqs_batch);
                    chorus::end_stat_tracking!(start);
                }

                // getting owndership back of server_state_static
                server_state_static = server_state;
            },
            BenchmarkType::Client => {
                let committee_1 = read_from_file!(&dir_name, "committee_1", CommitteeData);
                let committee_2 = read_from_file!(&dir_name, "committee_2", CommitteeData);
                let committee_3 = read_from_file!(&dir_name, "committee_3", CommitteeData);
                let prev_state = read_from_file!(&dir_name, "commstate_1_seat_idx", CommitteeStateClient);
                let commstate_2 = read_from_file!(&dir_name, "commstate_2_seat_idx", CommitteeStateClient);
                let public_state_epoch_3 = read_from_file!(&dir_name, "public_state_epoch_3", PublicState);
                let typical_handover_id = read_from_file!(&dir_name, "typical_handover_id", usize);
                let mut typical_handover_client_state = ECPSSClient::new(typical_handover_id, params.clone());

                // reqs_batch
                let mut server_state = ServerState::new(params.clone());
                let backup_ciphertext = read_from_file!(&dir_name, "backup_ciphertext", BackupCiphertext);
                let reqs_batch = read_from_file!(&dir_name, "requests_batch", RecoveryRequestBatch);
                server_state.store_backup(typical_handover_id, &backup_ciphertext);
                let recovery_response = read_from_file!(&dir_name, "recovery_response", RecoveryResponse);

                let start = chorus::start_stat_tracking!();
                println!("> Bench Handover (Typical) for a client that does handover");
                bench_handover_typical(c, &mut typical_handover_client_state, &public_state_epoch_3, &committee_1, &committee_2, &committee_3, Some(&prev_state.coeff_cmts), &commstate_2, Some(&reqs_batch), &params, 3);
                // bench_handover_typical_with_network(c, &mut typical_handover_client_state, &params, 3, case);
                chorus::end_stat_tracking!(start);

                let pwd = read_from_file!(&dir_name, "pwd", [u8; PWD_LEN]);
                let backup_key = read_from_file!(&dir_name, "backup_key", [u8; 32]);
                let mut sr_client = SecretRecoveryClient::new(typical_handover_id, &ibe_pk, params.clone());
                let start = chorus::start_stat_tracking!();
                println!("> Bench Backup for a key recovery client");
                bench_backup(c, &mut sr_client, &pwd.to_vec(), &backup_key.to_vec());
                chorus::end_stat_tracking!(start);

                let start = chorus::start_stat_tracking!();
                println!("> Bench Recovery Request for a key recovery client");
                bench_recovery_request(c, &mut sr_client, &pwd.to_vec());
                chorus::end_stat_tracking!(start);

                let blind = read_from_file!(&dir_name, "blind", Blind);
                let start = chorus::start_stat_tracking!();
                println!("> Bench Recover for a key recovery client");
                // bench_recover(c, &mut sr_client, &recovery_response, &blind, &pwd.to_vec());
                chorus::end_stat_tracking!(start);

                server_state_static = None;
            }
        }
    }
    }
}

#[cfg(feature = "deterministic")]
criterion_group! {
    name = benches;
    config = custom_config();
    targets = benches_class
}
criterion_main!(benches);
