#[macro_use]
extern crate criterion;

use class_group::{cl_hsm::random_integer_below, CL_HSM_Ciphertext, CL_HSM_PublicKey, CL_HSM_SecretKey, ClassGroup, CL_HSM, QFI, Integer};
use criterion::Criterion;
use rand::thread_rng;
use rug::{rand::RandState};

fn benches_class(c: &mut Criterion) {
    let bench_setup = |c: &mut Criterion, q: &Integer, seed: &Integer, seclevel: usize| {
        c.bench_function(&format!("setup"), move |b| {
            b.iter(|| CL_HSM::new(q, seed, seclevel))
        });
    };

    let bench_nudupl = |c: &mut Criterion, cl: &CL_HSM, f: &QFI| {
        c.bench_function(&format!("nudupl"), move |b| {
            b.iter(|| cl.nudupl(f))
        });
    };

    let bench_nucomp = |c: &mut Criterion, cl: &CL_HSM, f1: &QFI, f2: &QFI| {
        c.bench_function(&format!("nucomp"), move |b| {
            b.iter(|| cl.nucomp(f1, f2))
        });
    };

    let bench_nupow = |c: &mut Criterion, cl: &CL_HSM, f: &QFI, n: &Integer| {
        c.bench_function(&format!("nupow"), move |b| {
            b.iter(|| cl.nupow(f, n))
        });
    };

    let bench_power_of_h = |c: &mut Criterion, cl: &CL_HSM, n: &Integer| {
        c.bench_function(&format!("power_of_h"), move |b| {
            b.iter(|| cl.power_of_h(n))
        });
    };

    let bench_power_of_f = |c: &mut Criterion, cl: &CL_HSM, m: &Integer| {
        c.bench_function(&format!("power_of_f"), move |b| {
            b.iter(|| cl.power_of_f(m))
        });
    };

    let bench_keygen = |c: &mut Criterion, cl: &CL_HSM| {
        c.bench_function(&format!("keygen"), move |b| {

            b.iter(|| cl.keygen(&mut RandState::new()))
        });
    };

    let bench_precompute_pk = |c: &mut Criterion, cl: &CL_HSM, pk: &CL_HSM_PublicKey| {
        c.bench_function(&format!("precompute_pk"), move |b| {
            b.iter(|| cl.precompute_pk(pk))
        });
    };

    let bench_encrypt = |c: &mut Criterion, cl: &CL_HSM, pk: &CL_HSM_PublicKey, m: &Integer| {
        c.bench_function(&format!("encrypt"), move |b| {
            b.iter(|| { assert!(pk.precomp.is_none()); cl.encrypt(pk, m) })
        });
    };

    let bench_encrypt_precomputed = |c: &mut Criterion, cl: &CL_HSM, pk: &CL_HSM_PublicKey, m: &Integer| {
        c.bench_function(&format!("encrypt with precomputed pk"), move |b| {
            b.iter(|| { assert!(pk.precomp.is_some()); cl.encrypt(pk, m) })
        });
    };

    let bench_decrypt = |c: &mut Criterion, cl: &CL_HSM, sk: &CL_HSM_SecretKey, ct: &CL_HSM_Ciphertext| {
        c.bench_function(&format!("decrypt"), move |b| {
            b.iter(|| cl.decrypt(sk, ct))
        });
    };

    // change below to `for &i in &[1_000, 2_000, 5_000, 10_000, 100_000, 1_000_000] {` if needed to expand test cases,
    // may also need to increase pari_init size (first parameter in `pari_init`) in src/primitives/vdf.rs
    for &i in &[1_0] {
        let q = Integer::from_str_radix("52435875175126190479447740508185965837690552500527637822603658699938581184513", 10).expect("Integer: from_str_radix failed");
        let seed = Integer::from_str_radix("42", 10).expect("Integer: from_str_radix failed");
        let seclevel: usize = 128;
        let cl = CL_HSM::new(&q, &seed, seclevel);
        let e = Integer::from_str_radix("5", 10).expect("Integer: from_str_radix failed");
        let h_e = cl.power_of_h(&e);
        let m = random_integer_below(&q);
        let f_m = cl.power_of_f(&m);
        let m_ = cl.dlog_in_F(&f_m);
        assert!(m_ == m);
        let n = random_integer_below(&cl.exponent_bound_);

        let mut rng = RandState::new();
        let (sk, pk) = cl.keygen(&mut rng);
        let pk_precomp = cl.precompute_pk(&pk);
        let m = Integer::from_str_radix("133713371337133713371337133713371337133713371337133713371337133713371337", 10).expect("Integer: from_str_radix failed");
        let (ct, _) = cl.encrypt(&pk, &m);
        let m_ = cl.decrypt(&sk, &ct);
        assert!(m_ == m);

        bench_setup(c, &q, &seed, seclevel);
        bench_nudupl(c, &cl, &h_e);
        bench_nucomp(c, &cl, &f_m, &h_e);
        bench_nupow(c, &cl, &h_e, &n);
        bench_power_of_h(c, &cl, &n);
        bench_power_of_f(c, &cl, &m);
        bench_keygen(c, &cl);
        bench_precompute_pk(c, &cl, &pk);
        bench_encrypt(c, &cl, &pk, &m);
        bench_encrypt_precomputed(c, &cl, &pk_precomp, &m);
        bench_decrypt(c, &cl, &sk, &ct);
    }
}

criterion_group!(benches, benches_class);
criterion_main!(benches);