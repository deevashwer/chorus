use crate::crypto::proofs::utils::field_to_integer;
use crate::crypto::proofs::{pocs::PoCS, utils::integer_to_field};
use crate::crypto::proofs::{
    FeldmanCommitment, PoCSInstance, PoCSProof, PoCSWitness
};
pub use crate::crypto::shamir::ShamirSecretSharing;
use crate::crypto::shamir::Sharing;
use ark_ec::pairing::Pairing;
use ark_std::rand::Rng;
use class_group::{
    CL_HSM_Ciphertext, CL_HSM_MRCiphertext, CL_HSM_PublicKey, CL_HSM_SecretKey, CL_HSM, Integer
};
use serde::{Deserialize, Serialize, de};
use ed25519_dalek::ed25519::signature::SignerMut;
use ed25519_dalek::{Signature, SigningKey, VerifyingKey};

pub use ark_bls12_377::Bls12_377 as E;
pub type G1Affine = <E as Pairing>::G1Affine;
pub type ScalarField = <E as Pairing>::ScalarField;

use super::error::NIVSSError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dealing {
    pub mr_ctxt: CL_HSM_MRCiphertext,
    pub coeff_cmt: Vec<FeldmanCommitment>,
    pub pocs_proof: PoCSProof,
    pub signature: Signature,
}

pub struct NIVSS {
    pub(crate) pocs: PoCS,
}

impl NIVSS {
    pub fn new(g_bar: &G1Affine, cl: &CL_HSM) -> Self {
        Self {
            pocs: PoCS::new(g_bar, cl),
        }
    }

    pub fn encrypt_and_prove(&self, sharing: &Sharing<ScalarField>, public_keys: &Vec<CL_HSM_PublicKey>, coeff_cmt: &Vec<FeldmanCommitment>, rng: &mut impl Rng) -> (CL_HSM_MRCiphertext, PoCSProof) {
        let sharing_integer = sharing
            .shares
            .iter()
            .map(|x| field_to_integer::<ScalarField>(x))
            .collect::<Vec<Integer>>();

        let (mr_ctxt, r) = self.pocs.cl.mr_encrypt(public_keys, &sharing_integer);

        let instance = PoCSInstance {
            public_keys: public_keys.clone(),
            mr_ctxt: mr_ctxt.clone(),
            coeff_cmt: coeff_cmt.clone(),
        };

        let witness = PoCSWitness {
            shares: sharing.clone(),
            enc_r: r,
        };

        let pocs_proof = self.pocs.prove(&instance, &witness, rng);

        (mr_ctxt, pocs_proof)
    }

    pub fn deal(
        &self,
        sid: &Vec<u8>,
        secret: &ScalarField,
        threshold: usize,
        public_keys: &Vec<CL_HSM_PublicKey>,
        sig_key: &mut SigningKey,
        rng: &mut impl Rng,
    ) -> Dealing {
        let num_shares = public_keys.len();
        assert!(num_shares >= threshold);
        let sharing =
            ShamirSecretSharing::<ScalarField>::new_sharing(&secret, num_shares, threshold, rng);
        let coeff_cmt = sharing
            .coeffs
            .iter()
            .map(|x| FeldmanCommitment::new(&self.pocs.g_bar, x))
            .collect::<Vec<FeldmanCommitment>>();

        let (mr_ctxt, pocs_proof) = self.encrypt_and_prove(&sharing, public_keys, &coeff_cmt, rng);

        // sign the dealing
        let mut msg = Vec::new();
        msg.extend_from_slice(&sid);
        msg.extend_from_slice(&bincode::serialize(&mr_ctxt).unwrap());
        msg.extend_from_slice(&bincode::serialize(&coeff_cmt).unwrap());
        let signature = sig_key.sign(&msg);

        Dealing {
            mr_ctxt,
            coeff_cmt,
            pocs_proof,
            signature
        }
    }

    pub fn verify_pocs(&self, dealing: &Dealing, threshold: usize, public_keys: &Vec<CL_HSM_PublicKey>) -> Result<(), NIVSSError> {
        assert!(dealing.coeff_cmt.len() == threshold);

        let instance = PoCSInstance {
            public_keys: public_keys.clone(),
            mr_ctxt: dealing.mr_ctxt.clone(),
            coeff_cmt: dealing.coeff_cmt.clone(),
        };
        self.pocs.verify(&instance, &dealing.pocs_proof)?;

        Ok(())
    }

    pub fn receive(&self, sid: &Vec<u8>, dealing: &Dealing, threshold: usize, public_keys: &Vec<CL_HSM_PublicKey>, sig_pubkey: &VerifyingKey, share_idx: usize, sk: &CL_HSM_SecretKey) -> Result<ScalarField, NIVSSError> {
        self.verify_pocs(dealing, threshold, public_keys)?;

        let mut msg = Vec::new();
        msg.extend_from_slice(&sid);
        msg.extend_from_slice(&bincode::serialize(&dealing.mr_ctxt).unwrap());
        msg.extend_from_slice(&bincode::serialize(&dealing.coeff_cmt).unwrap());
        sig_pubkey.verify_strict(&msg, &dealing.signature)?;

        let ctxt_c2 = &dealing.mr_ctxt.c2[share_idx];
        let ctxt_c1 = &dealing.mr_ctxt.c1;
        let ctxt = CL_HSM_Ciphertext {
            c1: ctxt_c1.clone(),
            c2: ctxt_c2.clone(),
        };
        let share = self.pocs.cl.decrypt(sk, &ctxt);
        Ok(integer_to_field(&share))
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use ark_bls12_377::Fr;
    use ark_ec::AffineRepr;
    use ark_ff::{BigInteger, MontConfig, UniformRand};
    use class_group::{CL_HSM, Integer};
    use rand::rngs::OsRng;
    use rug::{integer::Order, rand::RandState};
    type E = ark_bls12_377::Bls12_377;
    type G1 = <E as Pairing>::G1Affine;

    #[test]
    pub fn test_nivss() {
        let num_shares = 128;
        let threshold = 64;

        let mut rng = ark_std::test_rng();
        let secret = Fr::rand(&mut rng);

        let g_bar = G1::generator();
        let q = Integer::from_digits(
            &ark_bls12_377::fr::FrConfig::MODULUS.to_bytes_le(),
            Order::Lsf,
        );
        let seed = Integer::from_str_radix("42", 10).expect("Integer: from_str_radix failed");
        let cl = CL_HSM::new(&q, &seed, 128);

        let mut pke_pubkeys = Vec::new();
        let mut pke_seckeys = Vec::new();
        let mut keygen_rng = RandState::new();
        for _ in 0..num_shares {
            let (sk, pk) = cl.keygen(&mut keygen_rng);
            pke_seckeys.push(sk);
            pke_pubkeys.push(pk);
        }
        let mut csprng = OsRng::default();
        let mut sig_key = SigningKey::generate(&mut csprng);

        let nivss = NIVSS::new(&g_bar, &cl);
        let sid = b"TEST".to_vec();
        let dealing = nivss.deal(&sid, &secret, threshold, &pke_pubkeys, &mut sig_key, &mut rng);

        // test serialization
        let dealing_bytes = bincode::serialize(&dealing).unwrap();
        let dealing_de: Dealing = bincode::deserialize(&dealing_bytes).unwrap();
        assert_eq!(dealing, dealing_de);

        println!("Dealing bytesize: {}", bincode::serialized_size(&dealing).unwrap());
        println!("mr_ctxt bytesize: {}", bincode::serialized_size(&dealing.mr_ctxt).unwrap());
        println!("coeff_cmt bytesize: {}", bincode::serialized_size(&dealing.coeff_cmt).unwrap());
        println!("pocs_proof bytesize: {}", bincode::serialized_size(&dealing.pocs_proof).unwrap());
        println!("signature bytesize: {}", dealing.signature.to_bytes().len());
        let shares: Vec<Fr> = (0..num_shares)
            .map(|i| nivss.receive(&sid, &dealing, threshold, &pke_pubkeys, &sig_key.verifying_key(), i, &pke_seckeys[i]).expect("recieve failed")).collect();
        let secret_ = ShamirSecretSharing::<Fr>::recover_secret(&shares, None);
        assert!(secret == secret_);
    }
}
