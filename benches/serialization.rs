#[macro_use]
extern crate criterion;

use std::env;

use ark_bls12_377::G1Affine;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use chorus::{read_from_file, secret_recovery::common::{CoefficientCommitments, CommitteeData, CommitteeStateClient, HandoverLite}};
use criterion::Criterion;
use serde::{de::DeserializeOwned, Serialize};
use std::io::Read;

fn custom_config() -> Criterion {
    Criterion::default().sample_size(10) // Reduces the number of samples
}

const dir_name: &str = "case_1_clients_1M";

fn benchmark_deserialization<T>(c: &mut Criterion, data: &T)
where
    T: Serialize + DeserializeOwned,
{
    c.bench_function("deserialize", |b| {
        let bytes = bincode::serialize(&data).unwrap();
        b.iter(|| {
            let _ = bincode::deserialize::<T>(&bytes).unwrap();
        });
    });
}

fn benchmark_arkworks_serialization(c: &mut Criterion, data: &G1Affine)
{
    c.bench_function("deserialize uncompressed unchecked", |b| {
        let mut bytes = Vec::new();
        data.serialize_uncompressed(&mut bytes).unwrap();
        println!("uncompressed size: {:?}", bytes.len());
        b.iter(|| {
            let _ = G1Affine::deserialize_uncompressed_unchecked(&bytes[..]);
        });
    });
    c.bench_function("deserialize compressed unchecked", |b| {
        let mut bytes = Vec::new();
        data.serialize_compressed(&mut bytes).unwrap();
        println!("compressed size: {:?}", bytes.len());
        b.iter(|| {
            let _ = G1Affine::deserialize_compressed_unchecked(&bytes[..]);
        });
    });
}

fn benches_class(c: &mut Criterion) {
    let commstate_1 = read_from_file!(dir_name, "commstate_1_seat_idx", CommitteeStateClient);
    benchmark_deserialization::<CommitteeStateClient>(c, &commstate_1);
    // benchmark_arkworks_serialization(c, &commstate_1.coeff_cmts.coeff_cmts[0].cmt);
}

criterion_group! {
    name = benches;
    config = custom_config();
    targets = benches_class
}
criterion_main!(benches);