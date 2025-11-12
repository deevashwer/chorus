use std::marker::PhantomData;

use ark_ec::{pairing::Pairing, AffineRepr, VariableBaseMSM};
use ark_ff::field_hashers::{DefaultFieldHasher, HashToField};
use ark_ff::PrimeField;
use ark_serialize::{CanonicalSerialize, CanonicalDeserialize};
use ark_std::{rand::Rng, UniformRand, Zero};
use serde::{Serialize, Deserialize};
use sha2::Sha256;

pub use ark_bls12_377::Bls12_377 as E;
pub type G1Affine = <E as Pairing>::G1Affine;
pub type ScalarField = <E as Pairing>::ScalarField;

use crate::crypto::proofs::utils::g1_affine_to_bytes;

use super::error::VerificationError;
use super::pocs::FeldmanCommitment;

#[derive(Serialize, Deserialize)]
pub struct Instance {
    pub comm1: FeldmanCommitment,
    pub comm2: FeldmanCommitment,
}

pub struct Witness {
    pub opening: ScalarField,
}

// Naively batching Chaum-Pedersen proofs for now
pub struct ChaumPedersen {
    pub g: G1Affine,
    pub h: G1Affine,
    pub H: DefaultFieldHasher<Sha256>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Proof {
    pub a: G1Affine,
    pub b: G1Affine,
    pub s: ScalarField,
}

impl Serialize for Proof {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut bytes = Vec::new();

        self.a.serialize_compressed(&mut bytes).unwrap();
        self.b.serialize_compressed(&mut bytes).unwrap();
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
        let a = G1Affine::deserialize_compressed_unchecked(&mut cursor).unwrap();
        let b = G1Affine::deserialize_compressed_unchecked(&mut cursor).unwrap();
        let s = ScalarField::deserialize_compressed_unchecked(&mut cursor).unwrap();

        Ok(Self { a, b, s })
    }
}

impl ChaumPedersen {
    pub fn new(g: &G1Affine, h: &G1Affine) -> Self {
        let H = <DefaultFieldHasher<Sha256> as HashToField<ScalarField>>::new(b"gamma dleq");
        Self {
            g: g.clone(),
            h: h.clone(),
            H,
        }
    }

    pub fn prove(
        &self,
        instance: &Instance,
        witness: &Witness,
        rng: &mut impl Rng,
    ) -> Proof {
        let r: ScalarField = ScalarField::rand(rng);
        let a: G1Affine = self.g.mul_bigint(r.into_bigint()).into();
        let b: G1Affine = self.h.mul_bigint(r.into_bigint()).into();

        let mut bytes = Vec::new();
        bytes.extend(bincode::serialize(&instance).unwrap());
        bytes.extend(g1_affine_to_bytes::<E>(&a));
        bytes.extend(g1_affine_to_bytes::<E>(&b));
        let c: ScalarField = self.H.hash_to_field(&bytes, 1).pop().unwrap();
        let s = r + (c * &witness.opening);
        Proof { a, b, s }
    }

    pub fn verify(
        &self,
        instance: &Instance,
        proof: &Proof,
    ) -> Result<(), VerificationError> {
        let mut bytes = Vec::new();
        bytes.extend(bincode::serialize(&instance).unwrap());
        bytes.extend(g1_affine_to_bytes::<E>(&proof.a));
        bytes.extend(g1_affine_to_bytes::<E>(&proof.b));
        let c: ScalarField = self.H.hash_to_field(&bytes, 1).pop().unwrap();
        let c_bigint = c.into_bigint();

        let lhs: <E as Pairing>::G1 = self.g.mul_bigint(proof.s.into_bigint());
        let rhs: <E as Pairing>::G1 = instance.comm1.cmt.mul_bigint(c_bigint) + proof.a;
        if lhs != rhs {
            return Err(VerificationError::Proof(
                "Chaum-Pedersen Check 1 Failed".to_string(),
            ));
        }

        let lhs: <E as Pairing>::G1 = self.h.mul_bigint(proof.s.into_bigint());
        let rhs: <E as Pairing>::G1 = instance.comm2.cmt.mul_bigint(c_bigint) + proof.b;
        if lhs != rhs {
            return Err(VerificationError::Proof(
                "Chaum-Pedersen Check 2 Failed".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use ark_bls12_377::Fr;
    use ark_ec::CurveGroup;
    type E = ark_bls12_377::Bls12_377;
    type G1 = <E as Pairing>::G1Affine;

    #[test]
    pub fn test_dleq() {
        let g = G1::generator();
        let h = g
            .mul_bigint(Fr::rand(&mut ark_std::test_rng()).into_bigint())
            .into_affine();
        let mut rng = ark_std::test_rng();
        let witness: Witness = Witness {
            opening: Fr::rand(&mut rng),
        };
        let comm1 = FeldmanCommitment::new(&g, &witness.opening);
        let comm2 = FeldmanCommitment::new(&h, &witness.opening);
        let instance: Instance = Instance { comm1, comm2 };

        let dleq = ChaumPedersen::new(&g, &h);

        let proof = dleq.prove(&instance, &witness, &mut rng);
        dleq.verify(&instance, &proof).unwrap();

        // test serialization
        let proof_bytes = bincode::serialize(&proof).unwrap();
        let proof_deserialized: Proof = bincode::deserialize(&proof_bytes).unwrap();
        println!("proof len: {}", proof_bytes.len());
        assert_eq!(proof, proof_deserialized);
    }
}
