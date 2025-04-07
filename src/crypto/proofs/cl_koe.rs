use std::mem;

use class_group::{CL_HSM_PublicKey, CL_HSM_SecretKey, CL_HSM, Integer};
use num_bigint::{BigUint, RandomBits};
use rand::Rng;
use rug::{integer::Order};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{error::VerificationError, utils::bigint_to_integer};

pub const STATISTICAL_SECLEVEL: u32 = 128;
pub const SECLEVEL: u32 = 128;

// Knowledge of Exponent Proof for CL_HSM PublicKey
pub struct CLKoE {
    pub cl: CL_HSM,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Instance {
    pub public_key: CL_HSM_PublicKey,
}

pub struct Witness {
    pub secret_key: CL_HSM_SecretKey,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Proof {
    pub c: Integer,
    pub s: Integer,
}

impl CLKoE {
    pub fn new(cl: &CL_HSM) -> Self {
        Self { cl: cl.clone() }
    }

    pub fn prove(&self, instance: &Instance, witness: &Witness, rng: &mut impl Rng) -> Proof {
        let r_bits: u64 = (self.cl.q_.significant_bits()
            + STATISTICAL_SECLEVEL
            + self.cl.exponent_bound_.significant_bits()) as u64;
        let r_bigint: BigUint = rng.sample(RandomBits::new(r_bits));
        let r: Integer = bigint_to_integer(&r_bigint);

        let a = self.cl.power_of_h(&r);

        let mut bytes = Vec::new();
        bytes.extend(bincode::serialize(&self.cl.h_).unwrap());
        bytes.extend(bincode::serialize(&instance).unwrap());
        bytes.extend(bincode::serialize(&a).unwrap());
        let mut H = Sha256::new_with_prefix(b"cl_koe");
        H.update(bytes);
        let c_bytes: Vec<u8> = H.finalize().to_vec();
        let c = Integer::from_digits(&c_bytes, Order::Lsf);

        let s = r + &witness.secret_key.sk_ * &c;
        Proof { c, s }
    }

    pub fn verify(&self, instance: &Instance, proof: &Proof) -> Result<(), VerificationError> {
        let max_bits = self.cl.q_.significant_bits()
            + STATISTICAL_SECLEVEL
            + self.cl.exponent_bound_.significant_bits() + 1;
        
        if proof.s.significant_bits() > max_bits {
            return Err(VerificationError::Proof("CLKoE proof.s is too large".to_string()));
        }

        let mut a = self.cl.power_of_h(&proof.s);
        a = self.cl.nucompinv(&a, &self.cl.nupow(&instance.public_key.pk_, &proof.c));

        let mut bytes = Vec::new();
        bytes.extend(bincode::serialize(&self.cl.h_).unwrap());
        bytes.extend(bincode::serialize(&instance).unwrap());
        bytes.extend(bincode::serialize(&a).unwrap());
        let mut H = Sha256::new_with_prefix(b"cl_koe");
        H.update(bytes);
        let c_bytes: Vec<u8> = H.finalize().to_vec();
        let c = Integer::from_digits(&c_bytes, Order::Lsf);

        if c != proof.c {
            return Err(VerificationError::Proof("CLKoE proof.c does not match".to_string()));
        }

        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use class_group::{CL_HSM, Integer};
    use ark_ff::{MontConfig, BigInteger};
    use rug::{integer::Order, rand::RandState};

    #[test]
    pub fn test_koe() {
        let mut rng = ark_std::test_rng();
        let q = Integer::from_digits(
            &ark_bls12_377::fr::FrConfig::MODULUS.to_bytes_le(),
            Order::Lsf,
        );
        let seed = Integer::from_str_radix("42", 10).expect("Integer: from_str_radix failed");
        let cl = CL_HSM::new(&q, &seed, 128);
        let koe = CLKoE::new(&cl);

        let mut keygen_rng = RandState::new();
        let (sk, pk) = koe.cl.keygen(&mut keygen_rng);

        let instance = Instance {
            public_key: pk.clone(),
        };

        let witness = Witness {
            secret_key: sk.clone(),
        };

        let proof = koe.prove(&instance, &witness, &mut rng);
        koe.verify(&instance, &proof).unwrap();

        // test serialization
        let proof_bytes = bincode::serialize(&proof).unwrap();
        println!("proof_len: {:?}", proof_bytes.len());
        println!("pk len: {:?}", bincode::serialize(&pk).unwrap().len());
        let proof_de: Proof = bincode::deserialize(&proof_bytes).unwrap();
        assert_eq!(proof, proof_de);
    }
}
