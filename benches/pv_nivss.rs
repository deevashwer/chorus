#[macro_use]
extern crate criterion;

use ark_ec::pairing::Pairing;
use ark_ec::AffineRepr;
use ark_ff::{BigInteger, MontConfig};
use ark_std::{cfg_into_iter, UniformRand};
use class_group::{CL_HSM_PublicKey, CL_HSM_SecretKey, CL_HSM, Integer};
use criterion::Criterion;
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use rug::{integer::Order, rand::RandState};
use chorus::{cfg_into_iter_client, crypto::nivss::pv_nivss::{Dealing, NIVSS}, serialized_size};
type E = ark_bls12_377::Bls12_377;
type Fr = ark_bls12_377::Fr;
type G1 = <E as Pairing>::G1Affine;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

fn custom_config() -> Criterion {
    Criterion::default().sample_size(10) // Reduces the number of samples
}

const BENCH_CASES: [(usize, usize, usize, usize); 4] = [
    // (1, 5, 133, 338),
    // (2, 10, 189, 443),
    // (3, 20, 385, 817),
    // (4, 30, 919, 1854),
    // (5, 40, 3700, 7329),
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
                         dealing: &Dealing,
                         threshold: usize,
                         pke_pubkeys: &Vec<CL_HSM_PublicKey>,
                         sig_key: &mut SigningKey,
                         i: usize,
                         sk: &CL_HSM_SecretKey| {
        c.bench_function(&format!("receive"), move |b| {
            b.iter(||
                cfg_into_iter_client!(0..64).for_each(|_| {
                    nivss.receive(&sid, &dealing, threshold, &pke_pubkeys, &sig_key.verifying_key(), i, &sk).expect("recieve failed");
                })
            )
        });
    };

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

        let start = chorus::start_stat_tracking!();
        bench_dealing(c, &nivss, &sid, secret, threshold, &pke_pubkeys, &mut sig_key);
        chorus::end_stat_tracking!(start);

        let start = chorus::start_stat_tracking!();
        bench_receive(c, &nivss, &sid, &dealing, threshold, &pke_pubkeys, &mut sig_key, 0, &pke_seckeys[0]);
        chorus::end_stat_tracking!(start);
    }
}

criterion_group! {
    name = benches;
    config = custom_config();
    targets = benches_class
}
criterion_main!(benches);