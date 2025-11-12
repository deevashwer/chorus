use crate::{CL_HSM, QFI, ClassGroup, Integer};
use rug::{integer::Order, rand::{MutRandState, RandGen, RandState}};
use rug::Integer as RugInteger;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};

#[cfg(feature = "client-parallel")]
use rayon::prelude::*;

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

#[derive(Debug, Clone)]
pub struct SecretKey {
    pub sk_: Integer,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PublicKey {
    pub pk_: QFI,
    // Precomputation data: positive integers d_ and e_, and pk_^(2^e_), pk_^(2^d_), pk_^(d_+e_)
    pub precomp: Option<(usize, usize, QFI, QFI, QFI)>,
}

#[derive(Debug, Clone)]
pub struct EncryptionRandomness {
    pub r: Integer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ciphertext {
    pub c1: QFI,
    pub c2: QFI,
}

// Multi-recipient Ciphertext
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MRCiphertext {
    pub c1: QFI,
    pub c2: Vec<QFI>,
}

// TODO: make cryptographically secure
// randstate is using MT hash which is not cryptographically secure
pub fn random_integer_below(bound: &Integer) -> Integer {
    let mut os_rng = OsRng::default();
    let mut rng = RandState::new();
    let seed = RugInteger::from(os_rng.next_u64() as i64);
    rng.seed(&seed);
    bound.x.clone().random_below(&mut rng).into()
}

impl CL_HSM {
    pub fn keygen<R: MutRandState>(&self, rng: &mut R) -> (SecretKey, PublicKey) {
        let sk_ = self.exponent_bound_.x.clone().random_below(rng).into();
        let pk_ = self.power_of_h(&sk_);
        (SecretKey { sk_ }, PublicKey { pk_, precomp: None })
    }

    pub fn precompute_pk(&self, pk: &PublicKey) -> PublicKey {
        assert!(self.compact_variant_ == false);
        let d_: usize = ((self.exponent_bound_.significant_bits() + 1)/2) as usize;
        let e_: usize = d_/2 + 1;

        let mut pk_de_precomp_ = pk.pk_.clone();
        let mut pk_e_precomp_: QFI = Default::default();
        let mut pk_d_precomp_: QFI = Default::default();
        for i in 0..(d_+e_) {
            if i == e_ {
                pk_e_precomp_ = pk_de_precomp_.clone();
            }
            if i == d_ {
                pk_d_precomp_ = pk_de_precomp_.clone();
            }
            pk_de_precomp_ = self.nudupl(&pk_de_precomp_);
        }
        PublicKey { pk_: pk.pk_.clone(), precomp: Some((d_, e_, pk_e_precomp_, pk_d_precomp_, pk_de_precomp_)) }
    }

    fn encrypt_core(&self, pk: &PublicKey, m: &Integer, r: &Integer) -> QFI {
        assert!(m < &self.M_ && m >= &RugInteger::from(0).into());
        assert!(self.compact_variant_ == false);

        let f_m = self.power_of_f(&m);
        let pk_r = {
            if pk.precomp.is_some() {
                let precomp: &(usize, usize, QFI, QFI, QFI) = &pk.precomp.as_ref().unwrap();
                self.nupow_wprecomp(&pk.pk_, &r, precomp.0, precomp.1, &precomp.2, &precomp.3, &precomp.4)
            } else {
                self.nupow(&pk.pk_, &r)

            }
        };
        // c2 = f^m*pk^r
        let c2 = self.nucomp(&f_m, &pk_r);
        c2
    }

    pub fn encrypt(&self, pk: &PublicKey, m: &Integer) -> (Ciphertext, EncryptionRandomness) {
        // let mut rng = RandState::new();
        // let r = self.exponent_bound_.clone().random_below(&mut rng);
        let r = random_integer_below(&self.exponent_bound_);
        // c1 = h^r
        let c1 = self.power_of_h(&r);
        let c2 = self.encrypt_core(pk, m, &r);
        (Ciphertext { c1, c2 }, EncryptionRandomness { r })
    }

    pub fn mr_encrypt(&self, pk: &Vec<PublicKey>, m: &Vec<Integer>) -> (MRCiphertext, EncryptionRandomness) {
        // let mut rng = RandState::new();
        // let r = self.exponent_bound_.clone().random_below(&mut rng);
        let r = random_integer_below(&self.exponent_bound_);
        // c1 = h^r
        let c1 = self.power_of_h(&r);
        let pk_and_m = pk.iter().zip(m).map(|(pk_, m_)| (pk_, m_)).collect::<Vec<(&PublicKey, &Integer)>>();
        let c2 = cfg_into_iter_client!(pk_and_m).map(|(pk_, m_)| self.encrypt_core(pk_, m_, &r)).collect();
        (MRCiphertext { c1, c2 }, EncryptionRandomness { r })
    }

    pub fn decrypt(&self, sk: &SecretKey, ct: &Ciphertext) -> Integer {
        assert!(self.compact_variant_ == false);
        assert!(&sk.sk_ < &self.exponent_bound_ && &sk.sk_ >= &RugInteger::from(0).into());
        let c_1_sk = self.nupow(&ct.c1, &sk.sk_);
        // f_m = c_2 / c_1^sk
        let f_m = self.nucompinv(&ct.c2, &c_1_sk);
        let m = self.dlog_in_F(&f_m);
        m
    }

    pub fn nucomp(&self, f1: &QFI, f2: &QFI) -> QFI {
        self.Cl_Delta_.nucomp(f1, f2)
    }
    
    pub fn nucompinv(&self, f1: &QFI, f2: &QFI) -> QFI {
        self.Cl_Delta_.nucompinv(f1, f2)
    }

    pub fn nudupl(&self, f: &QFI) -> QFI {
        self.Cl_Delta_.nudupl(f)
    }

    pub fn nudupl_n(&self, f: &QFI, niter: usize) -> QFI {
        self.Cl_Delta_.nudupl_n(f, niter)
    }

    pub fn nupow(&self, f: &QFI, n: &Integer) -> QFI {
        self.Cl_Delta_.nupow(f, n)
    }

    pub fn nupow_wprecomp(&self, f: &QFI, n: &Integer, d: usize, e: usize, fe: &QFI, fd: &QFI, fed: &QFI) -> QFI {
        self.Cl_Delta_.nupow_wprecomp(f, n, d, e, fe, fd, fed)
    }
}