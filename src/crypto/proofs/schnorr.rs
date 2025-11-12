use std::marker::PhantomData;

use ark_ec::{pairing::Pairing, AffineRepr, VariableBaseMSM};
use ark_ff::field_hashers::{DefaultFieldHasher, HashToField};
use ark_ff::PrimeField;
use ark_serialize::{CanonicalSerialize, CanonicalDeserialize};
use ark_std::{rand::Rng, UniformRand, Zero};
use sha2::Sha256;
use serde::{Deserialize, Serialize};

use crate::crypto::proofs::utils::g1_affine_to_bytes;

use super::error::VerificationError;
use super::pocs::FeldmanCommitment;
pub use ark_bls12_377::Bls12_377 as E;
pub type G1Affine = <E as Pairing>::G1Affine;
pub type ScalarField = <E as Pairing>::ScalarField;

#[derive(Serialize, Deserialize)]
pub struct Instance {
    pub comm: Vec<FeldmanCommitment>,
}

pub struct Witness {
    pub opening: Vec<ScalarField>,
}

pub struct BatchedSchnorr {
    pub g_bar: G1Affine,
    pub H: DefaultFieldHasher<Sha256>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Proof {
    pub x: G1Affine,
    pub s: ScalarField,
}

impl Serialize for Proof {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut bytes = Vec::new();

        self.x.serialize_compressed(&mut bytes).unwrap();
        self.s.serialize_compressed(&mut bytes).unwrap();

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
        let x = G1Affine::deserialize_compressed_unchecked(&mut cursor).unwrap();
        let s = ScalarField::deserialize_compressed_unchecked(&mut cursor).unwrap();

        Ok(Self { x, s })
    }
}

impl BatchedSchnorr {
    pub fn new(g_bar: &G1Affine) -> Self {
        let H = <DefaultFieldHasher<Sha256> as HashToField<ScalarField>>::new(b"gamma schnorr");
        Self {
            g_bar: g_bar.clone(),
            H,
        }
    }

    pub fn prove(
        &self,
        instance: &Instance,
        witness: &Witness,
        rng: &mut impl Rng,
    ) -> Proof {
        assert_eq!(instance.comm.len(), witness.opening.len());

        let r: ScalarField = ScalarField::rand(rng);
        let x: G1Affine = self.g_bar.mul_bigint(r.into_bigint()).into();

        let mut bytes = Vec::new();
        bytes.extend(bincode::serialize(&instance).unwrap());
        bytes.extend(g1_affine_to_bytes::<E>(&x));
        let c: ScalarField = self.H.hash_to_field(&bytes, 1).pop().unwrap();

        let n = instance.comm.len();
        let mut powers_of_c: Vec<ScalarField> = vec![c];
        for i in 1..n {
            powers_of_c.push(powers_of_c[i - 1] * &c);
        }
        let mut s = ScalarField::zero();
        for i in 0..n {
            s += witness.opening[i] * powers_of_c[i];
        }
        s += &r;
        Proof { x, s }
    }

    pub fn verify(
        &self,
        instance: &Instance,
        proof: &Proof,
    ) -> Result<(), VerificationError> {
        let mut bytes = Vec::new();
        bytes.extend(bincode::serialize(&instance).unwrap());
        bytes.extend(g1_affine_to_bytes::<E>(&proof.x));
        let c: ScalarField = self.H.hash_to_field(&bytes, 1).pop().unwrap();

        let n = instance.comm.len();
        let mut powers_of_c: Vec<ScalarField> = vec![c];
        for i in 1..n {
            powers_of_c.push(powers_of_c[i - 1] * &c);
        }
        let comms: Vec<G1Affine> = instance.comm.iter().map(|cmt| cmt.cmt).collect();
        let mut lhs: <E as Pairing>::G1 = <E as Pairing>::G1::msm(&comms, &powers_of_c).unwrap();
        lhs += proof.x;
        let rhs: <E as Pairing>::G1 = self.g_bar.mul_bigint(proof.s.into_bigint());
        if lhs != rhs {
            return Err(VerificationError::Proof("Schnorr Check Failed".to_string()));
        }
        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use ark_bls12_377::Fr;
    type E = ark_bls12_377::Bls12_377;
    type G1 = <E as Pairing>::G1Affine;

    #[test]
    pub fn test_schnorr() {
        let batch_size = 100;

        let mut rng = ark_std::test_rng();
        let witness: Witness = Witness {
            opening: (0..batch_size)
                .map(|_| Fr::rand(&mut rng))
                .collect::<Vec<Fr>>(),
        };
        let instance: Instance = Instance {
            comm: (0..batch_size)
                .map(|i| FeldmanCommitment::new(&G1::generator(), &witness.opening[i]))
                .collect::<Vec<FeldmanCommitment>>(),
        };

        let schnorr = BatchedSchnorr::new(&G1::generator());

        let proof = schnorr.prove(&instance, &witness, &mut rng);
        schnorr.verify(&instance, &proof).unwrap();

        // test serialization
        let proof_bytes = bincode::serialize(&proof).unwrap();
        let proof_deserialized: Proof = bincode::deserialize(&proof_bytes).unwrap();
        assert!(proof == proof_deserialized);
    }
}
