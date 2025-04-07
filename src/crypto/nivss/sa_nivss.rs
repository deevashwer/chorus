use std::io::Read;

use crate::crypto::proofs::FeldmanCommitment;

use super::error::NIVSSError;
use crate::crypto::proofs::{error::VerificationError, utils::integer_to_field};
use crate::crypto::proofs::{
    BatchedSchnorr, PoCSProof, SchnorrInstance,
    SchnorrProof, SchnorrWitness,
};
pub use crate::crypto::shamir::ShamirSecretSharing;
use ark_ec::{pairing::Pairing, VariableBaseMSM, AffineRepr};
use ark_std::rand::Rng;
use class_group::{
    CL_HSM_Ciphertext, CL_HSM_MRCiphertext, CL_HSM_PublicKey, CL_HSM_SecretKey, CL_HSM,
};
use ed25519_dalek::ed25519::signature::SignerMut;
pub use super::pv_nivss::{NIVSS as pvNIVSS, Dealing as pvNIVSSDealing};
use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use ark_std::{cfg_into_iter, One};
use serde::{Serialize, Deserialize, de};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize, Compress};

pub use ark_bls12_377::Bls12_377 as E;
pub type G1Affine = <E as Pairing>::G1Affine;
pub type ScalarField = <E as Pairing>::ScalarField;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Dealing {
    pub mr_ctxt: CL_HSM_MRCiphertext,
    pub coeff_cmt: Vec<FeldmanCommitment>,
    pub pocs_proof: PoCSProof,
    pub schnorr_proof: SchnorrProof,
    pub signatures: Vec<Signature>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DealingLite {
    pub ctxt: CL_HSM_Ciphertext,
    pub coeff_cmt: Vec<FeldmanCommitment>,
    pub schnorr_proof: SchnorrProof,
    pub signature: Signature,
}

pub struct NIVSS {
    pub(crate) pv_nivss: pvNIVSS,
    pub(crate) schnorr: BatchedSchnorr,
}

impl NIVSS {
    pub fn new(g_bar: &G1Affine, cl: &CL_HSM) -> Self {
        Self {
            pv_nivss: pvNIVSS::new(g_bar, cl),
            schnorr: BatchedSchnorr::new(g_bar),
        }
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
            .map(|x| FeldmanCommitment::new(&self.schnorr.g_bar, x))
            .collect::<Vec<FeldmanCommitment>>();

        let (mr_ctxt, pocs_proof) = self.pv_nivss.encrypt_and_prove(&sharing, public_keys, &coeff_cmt, rng);

        let instance = SchnorrInstance { comm: coeff_cmt.clone() };
        let witness = SchnorrWitness { opening: sharing.coeffs.clone() };
        let schnorr_proof = self.schnorr.prove(&instance, &witness, rng);

        // sign the dealing
        let signatures = (0..num_shares).map(|i| {
            let mut msg = Vec::new();
            let ctxt_c1 = mr_ctxt.c1.clone();
            let ctxt_c2 = mr_ctxt.c2[i].clone();
            let ctxt = CL_HSM_Ciphertext { c1: ctxt_c1, c2: ctxt_c2 };
            msg.extend_from_slice(&sid);
            msg.extend_from_slice(&i.to_le_bytes());
            msg.extend_from_slice(&bincode::serialize(&ctxt).unwrap());
            msg.extend_from_slice(&bincode::serialize(&coeff_cmt).unwrap());
            let signature = sig_key.sign(&msg);
            signature
        }).collect::<Vec<Signature>>();

        Dealing {
            mr_ctxt,
            coeff_cmt,
            pocs_proof,
            schnorr_proof,
            signatures
        }
    }

    pub fn verify_dealing(&self, sid: &Vec<u8>, dealing: &Dealing, threshold: usize, public_keys: &Vec<CL_HSM_PublicKey>, sig_pubkey: &VerifyingKey) -> Result<(), NIVSSError> {
        // let start = std::time::Instant::now();
        let pv_nivss_dealing = pvNIVSSDealing { mr_ctxt: dealing.mr_ctxt.clone(), coeff_cmt: dealing.coeff_cmt.clone(), pocs_proof: dealing.pocs_proof.clone(), signature: dealing.signatures[0].clone() };
        self.pv_nivss.verify_pocs(&pv_nivss_dealing, threshold, public_keys)?;
        // let end = std::time::Instant::now();
        // println!("sa_nivss.verify_pocs: {:?}", end.duration_since(start));

        // let start = std::time::Instant::now();
        let instance = SchnorrInstance { comm: dealing.coeff_cmt.clone() };
        self.schnorr.verify(&instance, &dealing.schnorr_proof)?;
        // let end = std::time::Instant::now();
        // println!("sa_nivss.verify_schnorr: {:?}", end.duration_since(start));

        let n = public_keys.len();
        assert!(dealing.signatures.len() == n);

        // let start = std::time::Instant::now();
        (0..n).map(|i| {
            let ctxt_c1 = dealing.mr_ctxt.c1.clone();
            let ctxt_c2 = dealing.mr_ctxt.c2[i].clone();
            let ctxt = CL_HSM_Ciphertext { c1: ctxt_c1, c2: ctxt_c2 };
            let signature = &dealing.signatures[i];
            let mut msg = Vec::new();
            msg.extend_from_slice(&sid);
            msg.extend_from_slice(&i.to_le_bytes());
            msg.extend_from_slice(&bincode::serialize(&ctxt).unwrap());
            msg.extend_from_slice(&bincode::serialize(&dealing.coeff_cmt).unwrap());
            sig_pubkey.verify_strict(&msg, signature)?;
            Ok(())
        }).collect::<Result<(), NIVSSError>>()?;
        // let end = std::time::Instant::now();
        // println!("sa_nivss.verify_signature: {:?}", end.duration_since(start));
        Ok(())
    }

    pub fn get_lite_dealings(&self, dealing: &Dealing) -> Vec<DealingLite> {
        let n = dealing.mr_ctxt.c2.len();
        (0..n).map(|i| {
            let ctxt_c1 = dealing.mr_ctxt.c1.clone();
            let ctxt_c2 = dealing.mr_ctxt.c2[i].clone();
            let ctxt = CL_HSM_Ciphertext { c1: ctxt_c1, c2: ctxt_c2 };
            DealingLite { ctxt, coeff_cmt: dealing.coeff_cmt.clone(), schnorr_proof: dealing.schnorr_proof.clone(), signature: dealing.signatures[i].clone() }
        }).collect::<Vec<DealingLite>>()
    }

    pub fn receive(
        &self,
        sid: &Vec<u8>,
        dealing: &DealingLite,
        threshold: usize,
        sig_pubkey: &VerifyingKey,
        share_idx: usize,
        sk: &CL_HSM_SecretKey,
        powers_of_idx: Option<Vec<ScalarField>>,
    ) -> Result<ScalarField, NIVSSError> {
        assert!(dealing.coeff_cmt.len() == threshold);

        // verify schnorr proof
        let instance = SchnorrInstance { comm: dealing.coeff_cmt.clone() };
        self.schnorr.verify(&instance, &dealing.schnorr_proof)?;

        // decrypt ctxt
        let share = self.pv_nivss.pocs.cl.decrypt(sk, &dealing.ctxt);
        let share_field = integer_to_field(&share);
        
        // verify share aginst commitment
        let share_cmt_expected = FeldmanCommitment::new(&self.schnorr.g_bar, &share_field);
        let powers_of_idx = if powers_of_idx.is_some() {
            powers_of_idx.unwrap()
        } else {
            let mut powers_of_idx = vec![<E as Pairing>::ScalarField::one(); threshold];
            // share_idx is indexed from 0
            let val = <E as Pairing>::ScalarField::from((share_idx + 1) as u64);
            for j in 1..threshold {
                powers_of_idx[j] = powers_of_idx[j - 1] * &val;
            }
            powers_of_idx
        };
        let bases = dealing.coeff_cmt.iter().map(|cmt| cmt.cmt).collect::<Vec<_>>();
        let share_cmt = FeldmanCommitment { cmt: <E as Pairing>::G1::msm(&bases, &powers_of_idx).unwrap().into() };
        if share_cmt != share_cmt_expected {
            return Err(NIVSSError::ShareCommitmentsMismatch);
        }

        // verify signature
        let mut msg = Vec::new();
        msg.extend_from_slice(&sid);
        msg.extend_from_slice(&share_idx.to_le_bytes());
        msg.extend_from_slice(&bincode::serialize(&dealing.ctxt).unwrap());
        msg.extend_from_slice(&bincode::serialize(&dealing.coeff_cmt).unwrap());
        sig_pubkey.verify_strict(&msg, &dealing.signature)?;

        Ok(share_field)
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use ark_bls12_377::Fr;
    use ark_ec::AffineRepr;
    use ark_ff::{BigInteger, MontConfig, UniformRand};
    use class_group::{CL_HSM, Integer};
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;
    use rug::{integer::Order, rand::RandState};
    type E = ark_bls12_377::Bls12_377;
    type G1 = <E as Pairing>::G1Affine;

    #[test]
    pub fn test_nivss() {
        let num_shares = 1090;
        let threshold = 300;

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
        println!("Dealing bytesize: {}", bincode::serialized_size(&dealing).unwrap());
        println!("mr_ctxt bytesize: {}", bincode::serialized_size(&dealing.mr_ctxt).unwrap());
        println!("coeff_cmt bytesize: {}", bincode::serialized_size(&dealing.coeff_cmt).unwrap());
        println!("pocs_proof bytesize: {}", bincode::serialized_size(&dealing.pocs_proof).unwrap());
        println!("schnorr_proof bytesize: {}", bincode::serialized_size(&dealing.schnorr_proof).unwrap());
        println!("signature bytesize: {}", bincode::serialized_size(&dealing.signatures).unwrap());

        nivss.verify_dealing(&sid, &dealing, threshold, &pke_pubkeys, &sig_key.verifying_key()).expect("process_dealing failed");
        let lite_dealings = nivss.get_lite_dealings(&dealing);
        println!("Max Lite Dealing bytesize: {}", lite_dealings.iter().map(|x| bincode::serialized_size(&x).unwrap()).max().unwrap());
        println!("ctxt bytesize: {}", bincode::serialized_size(&lite_dealings[0].ctxt).unwrap());
        println!("coeff_cmt bytesize: {}", bincode::serialized_size(&lite_dealings[0].coeff_cmt).unwrap());
        println!("schnorr_proof bytesize: {}", bincode::serialized_size(&lite_dealings[0].schnorr_proof).unwrap());
        println!("signature bytesize: {}", bincode::serialized_size(&lite_dealings[0].signature).unwrap());

        // test serialization
        lite_dealings.iter().for_each(|lite_dealing| {
            let bytes = bincode::serialize(&lite_dealing).unwrap();
            let lite_dealing_: DealingLite = bincode::deserialize(&bytes).unwrap();
            assert_eq!(lite_dealing, &lite_dealing_);
        });

        let shares: Vec<Fr> = (0..num_shares)
            .map(|i| nivss.receive(&sid, &lite_dealings[i], threshold, &sig_key.verifying_key(), i, &pke_seckeys[i], None).expect("recieve failed")).collect();
        let secret_ = ShamirSecretSharing::<Fr>::recover_secret(&shares, None);
        assert!(secret == secret_);
    }
}
