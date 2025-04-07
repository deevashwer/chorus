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
    Criterion::default().sample_size(10) // Reduces the number of samples
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

const BENCH_CASES: [(usize, usize, usize, usize); 4] = [
    (1, 1, 105, 561),
    (2, 10, 300, 1090),
    (3, 19, 719, 2123),
    (4, 27, 1659, 4292),
];

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
                         sk: &CL_HSM_SecretKey| {
        c.bench_function(&format!("receive"), move |b| {
            b.iter(||
                cfg_into_iter_client!((0..64)).for_each(|_| {
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

    for (case, f, t, n) in BENCH_CASES.iter() {
        let case = *case;
        let f = *f;
        let threshold = *t;
        let committee_size = *n;
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
            bench_receive(c, &nivss, &sid, &lite_dealings[0], threshold, &mut sig_key, 0, &pke_seckeys[0]);
            chorus::end_stat_tracking!(start);
        } else if benchmark_type == BenchmarkType::Server {
            let start = chorus::start_stat_tracking!();
            bench_verify_dealing(c, &nivss, &sid, &dealing, threshold, &pke_pubkeys, &mut sig_key, max_dealings_needed);
            chorus::end_stat_tracking!(start);
        }
    }
}

criterion_group! {
    name = benches;
    config = custom_config();
    targets = benches_class
}
criterion_main!(benches);