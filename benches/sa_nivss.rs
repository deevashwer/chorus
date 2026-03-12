#[macro_use]
extern crate criterion;

use std::env;

use ark_ec::pairing::Pairing;
use ark_ec::AffineRepr;
use ark_ff::{BigInteger, MontConfig};
use ark_std::{cfg_iter, UniformRand};
use chorus::serialized_size;
use class_group::{CL_HSM_PublicKey, CL_HSM_SecretKey, CL_HSM, Integer};
use criterion::Criterion;
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use rug::{integer::Order, rand::RandState};
use chorus::{cfg_into_iter_client, crypto::nivss::sa_nivss::{Dealing, DealingLite, NIVSS}};
type E = ark_bls12_377::Bls12_377;
type Fr = ark_bls12_377::Fr;
type G1 = <E as Pairing>::G1Affine;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

fn custom_config() -> Criterion {
    let config = load_config();
    let sample_size = config["sample_size"].as_u64().unwrap() as usize;
    Criterion::default().sample_size(sample_size)
}

#[derive(PartialEq)]
enum BenchmarkType {
    Server,
    Client,
}

impl BenchmarkType {
    fn from_str(input: &str) -> Option<Self> {
        match input {
            "SERVER" => Some(BenchmarkType::Server),
            "CLIENT" => Some(BenchmarkType::Client),
            _ => None,
        }
    }
}

/// Reads config.json from the repo root and returns it as a serde_json::Value.
fn load_config() -> serde_json::Value {
    let config_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("config.json");
    let data = std::fs::read_to_string(&config_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", config_path.display(), e));
    serde_json::from_str(&data)
        .unwrap_or_else(|e| panic!("Failed to parse {}: {}", config_path.display(), e))
}

fn nivss_cases_from_config(config: &serde_json::Value) -> Vec<(usize, usize, usize, usize)> {
    config["nivss_cases"].as_array().expect("nivss_cases must be an array")
        .iter()
        .map(|c| {
            let case = c["case"].as_u64().unwrap() as usize;
            let fraction = c["fraction"].as_u64().unwrap() as usize;
            let threshold = c["threshold"].as_u64().unwrap() as usize;
            let committee_size = c["committee_size"].as_u64().unwrap() as usize;
            (case, fraction, threshold, committee_size)
        })
        .collect()
}

fn benches_class(c: &mut Criterion) {
    let bench_dealing = |c: &mut Criterion,
                         nivss: &NIVSS,
                         sid: &Vec<u8>,
                         secret: Fr,
                         threshold: usize,
                         pke_pubkeys: &Vec<CL_HSM_PublicKey>,
                         sig_key: &mut SigningKey| {
        c.bench_function(&format!("deal"), move |b| {
            let mut rng = ark_std::test_rng();
            b.iter(|| nivss.deal(&sid, &secret, threshold, &pke_pubkeys, sig_key, &mut rng))
        });
    };

    let bench_receive = |c: &mut Criterion,
                         nivss: &NIVSS,
                         sid: &Vec<u8>,
                         dealing: &DealingLite,
                         threshold: usize,
                         sig_key: &mut SigningKey,
                         i: usize,
                         sk: &CL_HSM_SecretKey,
                         receive_parallel: usize| {
        c.bench_function(&format!("receive"), move |b| {
            b.iter(||
                cfg_into_iter_client!((0..receive_parallel)).for_each(|_| {
                    nivss.receive(&sid, &dealing, threshold, &sig_key.verifying_key(), i, &sk, None).expect("receive failed");
                })
            )
        });
    };

    let bench_verify_dealing = |c: &mut Criterion,
                        nivss: &NIVSS,
                        sid: &Vec<u8>,
                        dealing: &Dealing,
                        threshold: usize,
                        pke_pubkeys: &Vec<CL_HSM_PublicKey>,
                        sig_pubkey: &mut SigningKey,
                        max_dealings_needed: usize| {
        c.bench_function(&format!("verify-dealing"), move |b| {
            b.iter(||
                (0..max_dealings_needed).into_par_iter().for_each(|_| {
                    nivss.verify_dealing(sid, dealing, threshold, pke_pubkeys, &sig_pubkey.verifying_key()).expect("verify_dealing failed")
                })
            )
        });
    };

    let benchmark_type_str = env::var("BENCHMARK_TYPE").expect("BENCHMARK_TYPE not found");
    let benchmark_type = BenchmarkType::from_str(&benchmark_type_str).unwrap();

    let g_bar = G1::generator();
    let q = Integer::from_digits(
        &ark_bls12_377::fr::FrConfig::MODULUS.to_bytes_le(),
        Order::Lsf,
    );
    let seed = Integer::from_str_radix("42", 10).expect("Integer: from_str_radix failed");
    let cl = CL_HSM::new(&q, &seed, 128);

    let config = load_config();
    let bench_cases = nivss_cases_from_config(&config);
    let receive_parallel = config["nivss_receive_parallel"].as_u64().unwrap() as usize;

    for &(case, f, threshold, committee_size) in &bench_cases {
        println!("case: {}, fraction: {}, committee_size: {}, threshold: {}", case, f, committee_size, threshold);

        let mut rng = ark_std::test_rng();
        let secret = Fr::rand(&mut rng);

        let mut pke_pubkeys = Vec::new();
        let mut pke_seckeys = Vec::new();
        let mut keygen_rng = RandState::new();
        for _ in 0..committee_size {
            let (sk, pk) = cl.keygen(&mut keygen_rng);
            pke_seckeys.push(sk);
            pke_pubkeys.push(pk);
        }
        let mut csprng = OsRng::default();
        let mut sig_key = SigningKey::generate(&mut csprng);

        let nivss = NIVSS::new(&g_bar, &cl);
        let sid = b"TEST".to_vec();
        // println!("Dealing memory usage: {}", measure_memory(|args| nivss.deal(args.0, args.1, args.2, args.3, args.4, args.5) , (&sid, &secret, threshold, &pke_pubkeys, &mut sig_key, &mut rng)));
        let dealing = nivss.deal(&sid, &secret, threshold, &pke_pubkeys, &mut sig_key, &mut rng);
        println!("Dealing bytesize: {}", serialized_size!(dealing));

        let lite_dealings = nivss.get_lite_dealings(&dealing);
        println!("Total Lite Dealing bytesize: {}", lite_dealings.iter().map(|d| serialized_size!(d)).sum::<usize>());
        println!("Max Lite Dealing bytesize: {}", lite_dealings.iter().map(|d| serialized_size!(d)).max().unwrap());
        println!("Lite Dealing len: {}", lite_dealings.len());

        let expected_online_committee_members = ((committee_size as f64) * (50.0 as f64 / 100.0)).ceil() as usize;
        let max_dealings_needed = 2 * threshold;
        println!("Expected online committee members: {}, max_dealings_needed: {}", expected_online_committee_members, max_dealings_needed);
        if benchmark_type == BenchmarkType::Client {
            let start = chorus::start_stat_tracking!();
            bench_dealing(c, &nivss, &sid, secret, threshold, &pke_pubkeys, &mut sig_key);
            chorus::end_stat_tracking!(start);

            let start = chorus::start_stat_tracking!();
            bench_receive(c, &nivss, &sid, &lite_dealings[0], threshold, &mut sig_key, 0, &pke_seckeys[0], receive_parallel);
            chorus::end_stat_tracking!(start);
        } else if benchmark_type == BenchmarkType::Server {
            let start = chorus::start_stat_tracking!();
            bench_verify_dealing(c, &nivss, &sid, &dealing, threshold, &pke_pubkeys, &mut sig_key, max_dealings_needed);
            chorus::end_stat_tracking!(start);
        }
    }
    println!("CHORUS_BENCHMARK_OK");
}

criterion_group! {
    name = benches;
    config = custom_config();
    targets = benches_class
}
criterion_main!(benches);