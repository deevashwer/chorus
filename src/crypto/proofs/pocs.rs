use std::env::var;
use std::marker::PhantomData;

use ark_bls12_377::g1;
use ark_ec::pairing::Pairing;
use ark_ec::AffineRepr;
use ark_ec::{Group, VariableBaseMSM};
use ark_ff::field_hashers::DefaultFieldHasher;
use ark_ff::field_hashers::HashToField;
use ark_ff::{BigInteger, UniformRand};
use ark_ff::{Field, PrimeField};
use ark_ff::{One, Zero};
use ark_serialize::{CanonicalSerialize, CanonicalDeserialize};
use ark_std::rand::Rng;
use class_group::{
    CL_HSM_Ciphertext, CL_HSM_EncryptionRandomness, CL_HSM_MRCiphertext, CL_HSM_PublicKey, CL_HSM,
    QFI, Integer
};
use num_bigint::{BigUint, RandomBits};
use rug::integer::Order;
use rug::rand::RandState;
use sha2::Sha256;
use tracing::field;
use serde::{de, Deserialize, Serialize};
pub use ark_bls12_377::Bls12_377 as E;
pub type G1Affine = <E as Pairing>::G1Affine;
pub type ScalarField = <E as Pairing>::ScalarField;

use crate::crypto::proofs::msm::{cl_msm, cl_msm_naive};
use crate::crypto::proofs::utils::{bigint_to_integer, field_to_integer};

use super::error::VerificationError;
use crate::crypto::shamir::Sharing;

pub const STATISTICAL_SECLEVEL: u32 = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeldmanCommitment {
    pub cmt: G1Affine,
}

impl Serialize for FeldmanCommitment {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut bytes = Vec::new();
        // self.cmt.serialize_compressed(&mut bytes).unwrap();
        self.cmt.serialize_compressed(&mut bytes).unwrap();
        serializer.serialize_bytes(&bytes)
    }
}

impl<'de> Deserialize<'de> for FeldmanCommitment {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        let cmt = G1Affine::deserialize_compressed_unchecked(&bytes[..]).unwrap();
        Ok(Self { cmt })
    }
}

impl FeldmanCommitment
where
    G1Affine: CanonicalSerialize + CanonicalDeserialize
{
    pub fn new(g: &G1Affine, msg: &ScalarField) -> Self {
        let cmt = g.mul_bigint(msg.into_bigint()).into();
        Self { cmt }
    }
}

pub struct PoCS {
    pub g_bar: G1Affine,
    pub cl: CL_HSM,
    pub H: DefaultFieldHasher<Sha256>,
    pub H_: DefaultFieldHasher<Sha256>,
}

#[derive(Serialize)]
pub struct Instance {
    pub public_keys: Vec<CL_HSM_PublicKey>,
    pub mr_ctxt: CL_HSM_MRCiphertext,
    pub coeff_cmt: Vec<FeldmanCommitment>,
}

pub struct Witness {
    pub shares: Sharing<ScalarField>,
    pub enc_r: CL_HSM_EncryptionRandomness,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Proof {
    pub W: QFI,
    pub X: G1Affine,
    pub Y: QFI,
    pub z_r: Integer,
    pub z_s: ScalarField,
}

impl Serialize for Proof {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut bytes = Vec::new();

        bytes.extend_from_slice(&bincode::serialize(&self.W).map_err(serde::ser::Error::custom)?);
        self.X.serialize_compressed(&mut bytes).unwrap();
        bytes.extend_from_slice(&bincode::serialize(&self.Y).map_err(serde::ser::Error::custom)?);
        bytes.extend_from_slice(&bincode::serialize(&self.z_r).map_err(serde::ser::Error::custom)?);
        self.z_s.serialize_compressed(&mut bytes).unwrap();

        serializer.serialize_bytes(&bytes)
    }
}

impl<'de> Deserialize<'de> for Proof {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        let mut cursor = std::io::Cursor::new(bytes);

        // Deserialize each field in order
        let W: QFI = bincode::deserialize_from(&mut cursor).map_err(de::Error::custom)?;
        let X = G1Affine::deserialize_compressed_unchecked(&mut cursor).unwrap();
        let Y: QFI = bincode::deserialize_from(&mut cursor).map_err(de::Error::custom)?;
        let z_r: Integer = bincode::deserialize_from(&mut cursor).map_err(de::Error::custom)?;
        let z_s = ScalarField::deserialize_compressed_unchecked(&mut cursor).unwrap();

        Ok(Self { W, X, Y, z_r, z_s })
    }
}

pub fn coeffs_and_shares_consistency_check_exponents<E: Pairing>(
    n: usize,
    t: usize,
    gamma: E::ScalarField,
) -> (Vec<E::ScalarField>, Vec<E::ScalarField>) {
    let mut powers_of_gamma = vec![gamma.clone(); n];
    for i in 1..n {
        powers_of_gamma[i] = powers_of_gamma[i - 1] * &gamma;
    }
    let mut powers_of_idx = Vec::new();
    for i in 1..=n {
        powers_of_idx.push(vec![E::ScalarField::one(); t]);
        let val = E::ScalarField::from(i as u64);
        for j in 1..t {
            powers_of_idx[i - 1][j] = powers_of_idx[i - 1][j - 1] * &val;
        }
    }
    let mut exponents = vec![E::ScalarField::zero(); t];
    for i in 0..t {
        for j in 0..n {
            exponents[i] += powers_of_idx[j][i] * &powers_of_gamma[j];
        }
    }
    (powers_of_gamma, exponents)
}

impl PoCS
{
    pub fn new(g_bar: &G1Affine, cl: &CL_HSM) -> Self {
        let H = <DefaultFieldHasher<Sha256> as HashToField<ScalarField>>::new(b"gamma PoCS");
        let H_ =
            <DefaultFieldHasher<Sha256> as HashToField<ScalarField>>::new(b"gamma prime PoCS");
        Self {
            g_bar: g_bar.clone(),
            cl: cl.clone(),
            H,
            H_,
        }
    }

    pub fn prove(
        &self,
        instance: &Instance,
        witness: &Witness,
        rng: &mut impl Rng,
    ) -> Proof {
        let alpha: ScalarField = ScalarField::rand(rng);

        let rho_bits: u64 = (ScalarField::MODULUS_BIT_SIZE
            + STATISTICAL_SECLEVEL
            + self.cl.exponent_bound_.significant_bits()) as u64;
        let rho_bigint: BigUint = rng.sample(RandomBits::new(rho_bits));
        let rho: Integer = bigint_to_integer(&rho_bigint);

        let W: QFI = self.cl.power_of_h(&rho);

        let X: G1Affine = self.g_bar.mul_bigint(alpha.into_bigint()).into();

        let gamma: ScalarField = self
            .H
            .hash_to_field(&bincode::serialize(&instance).unwrap(), 1)
            .pop()
            .unwrap();

        // let start = std::time::Instant::now();
        let n = instance.public_keys.len();
        let mut powers_of_gamma: Vec<ScalarField> = vec![gamma];
        for i in 1..n {
            powers_of_gamma.push(powers_of_gamma[i - 1] * &gamma);
        }
        let mut Y: QFI = self.cl.power_of_f(&field_to_integer(&alpha));
        let bases = instance
            .public_keys
            .iter()
            .map(|x| x.pk_.clone())
            .collect::<Vec<QFI>>();
        let scalars = powers_of_gamma
            .iter()
            .map(|x| field_to_integer(x))
            .collect::<Vec<Integer>>();
        // let mut pk_gamma: QFI = cl_msm_naive(&self.cl, &bases, &scalars);
        let mut pk_gamma: QFI = cl_msm(&self.cl, &bases, &scalars);
        pk_gamma = self.cl.nupow(&pk_gamma, &rho);
        Y = self.cl.nucomp(&Y, &pk_gamma);
        // let duration = start.elapsed();
        // println!("PoCS Prover - Y computation: {:?}", duration);

        let mut bytes = Vec::new();
        gamma.serialize_compressed(&mut bytes).unwrap();
        bytes.extend(bincode::serialize(&W).unwrap());
        X.serialize_compressed(&mut bytes).unwrap();
        bytes.extend(bincode::serialize(&Y).unwrap());
        let gamma_: ScalarField = self.H_.hash_to_field(&bytes, 1).pop().unwrap();

        let z_r: Integer = witness.enc_r.r.clone() * field_to_integer(&gamma_) + rho;

        let mut z_s: ScalarField = ScalarField::zero();
        for i in 0..n {
            z_s += witness.shares.shares[i] * &powers_of_gamma[i];
        }
        z_s *= &gamma_;
        z_s += &alpha;

        Proof { W, X, Y, z_r, z_s }
    }

    /*
    pub fn generate_lite_proof(&self, instance: &Instance<E>, proof: &Proof<E>) -> ProofLite<E> {
        let gamma: E::ScalarField = self.H.hash_to_field(&instance.to_bytes_le(), 1).pop().unwrap();

        let mut bytes = Vec::new();
        gamma.serialize_compressed(&mut bytes).unwrap();
        bytes.extend(proof.W.to_bytes_le());
        proof.X.serialize_compressed(&mut bytes).unwrap();
        bytes.extend(proof.Y.to_bytes_le());
        let gamma_: E::ScalarField = self.H_.hash_to_field(&bytes, 1).pop().unwrap();

        let gamma_inv = gamma_.inverse().unwrap();
        let X: E::G1Affine = proof.X.mul_bigint(gamma_inv.into_bigint()).into();
        let z_s = proof.z_s * &gamma_inv;
        ProofLite { X, z_s }
    }
    */

    pub fn verify(
        &self,
        instance: &Instance,
        proof: &Proof,
    ) -> Result<(), VerificationError> {
        let gamma: ScalarField = self
            .H
            .hash_to_field(&bincode::serialize(&instance).unwrap(), 1)
            .pop()
            .unwrap();

        let mut bytes = Vec::new();
        gamma.serialize_compressed(&mut bytes).unwrap();
        bytes.extend(bincode::serialize(&proof.W).unwrap());
        proof.X.serialize_compressed(&mut bytes).unwrap();
        bytes.extend(bincode::serialize(&proof.Y).unwrap());
        let gamma_: ScalarField = self.H_.hash_to_field(&bytes, 1).pop().unwrap();

        // first check
        let mut lhs = self
            .cl
            .nupow(&instance.mr_ctxt.c1, &field_to_integer(&gamma_));
        lhs = self.cl.nucomp(&lhs, &proof.W);
        let rhs = self.cl.power_of_h(&proof.z_r);
        if lhs != rhs {
            return Err(VerificationError::Proof(
                "PoCS First Check Failed".to_string(),
            ));
        }

        // let start = std::time::Instant::now();
        // second check
        let n = instance.public_keys.len();
        let t = instance.coeff_cmt.len();
        let (powers_of_gamma, exponents) =
            coeffs_and_shares_consistency_check_exponents::<E>(n, t, gamma);
        // let duration = start.elapsed();
        // println!("PoCS Verifier - exponent computation for second check: {:?}", duration);
        let coeff_cmt = instance
            .coeff_cmt
            .iter()
            .map(|x| x.cmt)
            .collect::<Vec<G1Affine>>();
        // let start = std::time::Instant::now();
        let mut lhs: <E as Pairing>::G1 = <E as Pairing>::G1::msm(&coeff_cmt, &exponents).unwrap();
        // let duration = start.elapsed();
        // println!("PoCS Verifier - msm for second check: {:?}", duration);
        lhs = lhs.mul_bigint(gamma_.into_bigint());
        lhs = lhs + &proof.X;
        let rhs = self.g_bar.mul_bigint(proof.z_s.into_bigint());
        if lhs != rhs {
            return Err(VerificationError::Proof(
                "PoCS Second Check Failed".to_string(),
            ));
        }

        // third check
        let bases = &instance.mr_ctxt.c2;
        let scalars = powers_of_gamma
            .iter()
            .map(|x| field_to_integer(x))
            .collect::<Vec<Integer>>();
        // let start = std::time::Instant::now();
        // let mut lhs = cl_msm_naive(&self.cl, bases, &scalars);
        let mut lhs = cl_msm(&self.cl, bases, &scalars);
        // let duration = start.elapsed();
        // println!("PoCS Verifier - LHS msm for third check: {:?}", duration);
        lhs = self.cl.nupow(&lhs, &field_to_integer(&gamma_));
        lhs = self.cl.nucomp(&lhs, &proof.Y);
        // let start = std::time::Instant::now();
        let bases = instance
            .public_keys
            .iter()
            .map(|x| x.pk_.clone())
            .collect::<Vec<QFI>>();
        // let mut rhs = cl_msm_naive(&self.cl, &bases, &scalars);
        let mut rhs = cl_msm(&self.cl, &bases, &scalars);
        // let duration = start.elapsed();
        // println!("PoCS Verifier - RHS msm for third check: {:?}", duration);
        rhs = self.cl.nupow(&rhs, &proof.z_r);
        let f_z_s = self.cl.power_of_f(&field_to_integer(&proof.z_s));
        rhs = self.cl.nucomp(&rhs, &f_z_s);
        if lhs != rhs {
            return Err(VerificationError::Proof(
                "PoCS Third Check Failed".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::crypto::proofs::utils::integer_to_field;
    use crate::crypto::shamir::ShamirSecretSharing;
    use ark_bls12_377::Fr;
    use ark_ff::{BigInteger, MontConfig};
    use class_group::{CL_HSM, Integer};
    use rug::integer::Order;
    type E = ark_bls12_377::Bls12_377;
    type G1 = <E as Pairing>::G1Affine;

    #[test]
    pub fn test_pocs() {
        let num_shares = 10;
        let threshold = 5;

        let mut rng = ark_std::test_rng();
        let secret = Fr::rand(&mut rng);
        let sharing =
            ShamirSecretSharing::<Fr>::new_sharing(&secret, num_shares, threshold, &mut rng);
        let sharing_integer = sharing
            .shares
            .iter()
            .map(|x| field_to_integer::<Fr>(x))
            .collect::<Vec<Integer>>();

        let g_bar = G1::generator();
        let q = Integer::from_digits(
            &ark_bls12_377::fr::FrConfig::MODULUS.to_bytes_le(),
            Order::Lsf,
        );
        let seed = Integer::from_str_radix("42", 10).expect("Integer: from_str_radix failed");
        let cl = CL_HSM::new(&q, &seed, 128);
        let pocs = PoCS::new(&g_bar, &cl);

        let mut secret_keys = Vec::new();
        let mut public_keys = Vec::new();

        let mut keygen_rng = RandState::new();
        for _ in 0..num_shares {
            let (sk, pk) = pocs.cl.keygen(&mut keygen_rng);
            secret_keys.push(sk);
            public_keys.push(pk);
        }
        let (mr_ctxt, r) = pocs.cl.mr_encrypt(&public_keys, &sharing_integer);

        let coeff_cmt = sharing
            .coeffs
            .iter()
            .map(|x| FeldmanCommitment::new(&g_bar, x))
            .collect::<Vec<FeldmanCommitment>>();

        let instance = Instance {
            public_keys: public_keys.clone(),
            mr_ctxt: mr_ctxt.clone(),
            coeff_cmt,
        };

        let witness = Witness {
            shares: sharing.clone(),
            enc_r: r,
        };

        let proof = pocs.prove(&instance, &witness, &mut rng);
        pocs.verify(&instance, &proof).unwrap();

        // test serialization
        let proof_bytes = bincode::serialize(&proof).unwrap();
        let proof_: Proof = bincode::deserialize(&proof_bytes).unwrap();
        assert_eq!(proof, proof_);
        println!("Serialization test passed");

        let ctxt = mr_ctxt
            .c2
            .iter()
            .map(|x| CL_HSM_Ciphertext {
                c1: mr_ctxt.c1.clone(),
                c2: x.clone(),
            })
            .collect::<Vec<CL_HSM_Ciphertext>>();
        let shares_ = ctxt
            .iter()
            .zip(secret_keys.iter())
            .map(|(ct, sk)| pocs.cl.decrypt(sk, ct))
            .collect::<Vec<Integer>>();
        for i in 0..num_shares {
            assert_eq!(sharing.shares[i], integer_to_field::<Fr>(&shares_[i]));
        }
    }
}
