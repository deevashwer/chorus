use std::borrow::Borrow;

use ark_ec::{bls12::{Bls12Config, G2Prepared}, hashing::curve_maps::wb::WBConfig, short_weierstrass::{Affine, Projective, SWCurveConfig}, CurveConfig};
use ark_ff::{BigInteger, Fp2Config, PrimeField};
use ark_r1cs_std::{alloc::AllocVar, eq::EqGadget, fields::FieldVar, groups::{bls12::G2PreparedVar, curves::short_weierstrass::{AffineVar, ProjectiveVar}, CurveVar}, prelude::Boolean, uint8::UInt8, R1CSVar, ToConstraintFieldGadget};
use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystemRef, SynthesisError};
use map_to_curve::{Fp2G, MapToCurveBasedHasherGadget};

pub mod hash_to_field;
pub mod map_to_curve;

// More work for user, less work for aggregator
#[derive(Clone)]
pub struct RecoveryRequestNIZK<P: Bls12Config> {
    pub request: Option<Affine<P::G2Config>>,
    pub client_id: Option<Vec<u8>>,
    pub pwd: Option<Vec<u8>>,
    pub blind: Option<<P::G2Config as CurveConfig>::ScalarField>,
    pub id_len: usize,
    pub pwd_len: usize,
}

impl<P: Bls12Config> ConstraintSynthesizer<P::Fp> for RecoveryRequestNIZK<P>
where P::G2Config: WBConfig
{
    fn generate_constraints(
        self,
        cs: ConstraintSystemRef<P::Fp>,
    ) -> Result<(), SynthesisError> {
        let (request_x, request_y) = match self.request {
            Some(r) => (Some(r.x), Some(r.y)),
            None => (None, None),
        };
        let request_x_var = Fp2G::<P>::new_input(cs.clone(), || request_x.ok_or(SynthesisError::AssignmentMissing))?;
        let request_y_var = Fp2G::<P>::new_input(cs.clone(), || request_y.ok_or(SynthesisError::AssignmentMissing))?;
        let request_var = ProjectiveVar::<P::G2Config, Fp2G<P>>::new(request_x_var, request_y_var, Fp2G::<P>::one());

        let client_id: Vec<u8> = match self.client_id {
            Some(c) => c,
            None => vec![0u8; self.id_len],
        };
        // let client_id_var = client_id.iter().map(|c| UInt8::<P::Fp>::new_input(cs.clone(), || c.ok_or(SynthesisError::AssignmentMissing))).collect::<Result<Vec<_>, SynthesisError>>()?;
        let client_id_var = UInt8::<P::Fp>::new_input_vec(cs.clone(), &client_id)?;
        assert!(client_id.len() == self.id_len);

        let pwd: Vec<Option<u8>> = match self.pwd {
            Some(c) => c.iter().map(|&c| Some(c)).collect(),
            None => vec![None; self.pwd_len],
        };
        let pwd_var = pwd.iter().map(|c| UInt8::<P::Fp>::new_witness(cs.clone(), || c.ok_or(SynthesisError::AssignmentMissing))).collect::<Result<Vec<_>, SynthesisError>>()?;
        assert!(pwd.len() == self.pwd_len);

        let blind_bits: Vec<Option<bool>> = match self.blind {
            Some(b) => b.into_bigint().to_bits_le().iter().map(|&b| Some(b)).collect(),
            None => vec![None; <P::G2Config as CurveConfig>::ScalarField::MODULUS_BIT_SIZE.try_into().unwrap()],
        };
        let blind_var: Vec<Boolean<P::Fp>> = blind_bits.iter()
                        .map(|b| Boolean::new_witness(cs.clone(), || b.ok_or(SynthesisError::AssignmentMissing)))
                        .collect::<Result<Vec<_>, SynthesisError>>()?;

        let ibe_id_var = client_id_var.into_iter().chain(pwd_var.into_iter()).collect::<Vec<_>>();

        let prefix = b"CHORUS-BF-TIBE-H1-G2";
        let prefix_var = UInt8::constant_vec(prefix);
        let hasher_var = MapToCurveBasedHasherGadget::<P>::new(&prefix_var)?;

        let hashed_id_var = hasher_var.hash(&ibe_id_var)?;
        let hashed_id_projective_var = ProjectiveVar::<P::G2Config, Fp2G<P>>::new(hashed_id_var.x, hashed_id_var.y, Fp2G::<P>::one());
        let expected_request_var = hashed_id_projective_var.scalar_mul_le(blind_var.iter())?;

        request_var.enforce_equal(&expected_request_var)?;

        if cs.is_in_setup_mode() {
            println!("Recovery Request Constraints: {}", cs.num_constraints());
            println!("Recovery Request Inputs: {}", cs.num_instance_variables());
            println!("Recovery Request Witnesses: {}", cs.num_witness_variables());
        } else {
            assert!(cs.is_satisfied().unwrap());
        }
        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use std::env;

    use ark_ec::bls12::Bls12;
    use ark_ec::{AffineRepr, CurveConfig, CurveGroup};
    use ark_ec::{bls12::Bls12Config, short_weierstrass::{Affine, SWCurveConfig}};
    use ark_relations::r1cs::{ConstraintSynthesizer, ConstraintSystem, OptimizationGoal};
    use ark_serialize::CanonicalSerialize;
    use ark_std::{end_timer, start_timer, test_rng};
    use rand::{thread_rng, Rng};
    use ark_bls12_377::Config as P;
    use ark_bls12_377::Bls12_377 as E1;
    use ark_bw6_761::BW6_761 as E2;
    use ark_ff::{PrimeField, ToConstraintField};
    use ark_crypto_primitives::snark::SNARK;
    use ark_groth16::Groth16;

    use crate::crypto::ibe::{self, BonehFranklinIBE, constraints::RecoveryRequestNIZK};
    type ConstraintF = <P as Bls12Config>::Fp;

    #[test]
    pub fn test_request_nizk() {
        // env::set_var("RAYON_NUM_THREADS", "1");

        let mut rng = thread_rng();

        const ID_LEN: usize = 32;
        const PWD_LEN: usize = 1;

        let client_id: [u8; ID_LEN] = rng.gen();
        let pwd: [u8; PWD_LEN] = rng.gen();
        let ibe_id: Vec<u8> = client_id.into_iter().chain(pwd.into_iter()).collect::<Vec<_>>();

        let hashed_id: Affine::<<P as Bls12Config>::G2Config> = BonehFranklinIBE::h1(&ibe_id).expect("Hashing failed");
        let blind: <<P as Bls12Config>::G2Config as CurveConfig>::ScalarField = rng.gen();
        let request = <P as Bls12Config>::G2Config::msm(&[hashed_id], &[blind]).unwrap().into_affine();

        let nizk_setup = RecoveryRequestNIZK::<P> {
            request: None,
            client_id: None,
            pwd: None,
            blind: None,
            id_len: ID_LEN,
            pwd_len: PWD_LEN,
        };
        let nizk_prove = RecoveryRequestNIZK::<P> {
            request: Some(request),
            client_id: Some(client_id.to_vec()),
            pwd: Some(pwd.to_vec()),
            blind: Some(blind),
            id_len: ID_LEN,
            pwd_len: PWD_LEN,
        };

        let groth_client_setup = start_timer!(|| "Groth Client Setup");
        let (groth_client_pk, groth_client_vk) =
            <Groth16<E2> as SNARK<ConstraintF>>::circuit_specific_setup(nizk_setup, &mut rng).unwrap();
        end_timer!(groth_client_setup);
        let mut compressed_pk = Vec::new();
        groth_client_pk.serialize_compressed(&mut compressed_pk).unwrap();
        println!("groth_client_pk size (in bytes): {}", compressed_pk.len());
        let mut compressed_vk = Vec::new();
        groth_client_vk.serialize_compressed(&mut compressed_vk).unwrap();
        println!("groth_client_vk size (in bytes): {}", compressed_vk.len());

        let groth_client_prove = start_timer!(|| "Groth Client Prove");
        let groth_client_proof =
            <Groth16<E2> as SNARK<ConstraintF>>::prove(&groth_client_pk, nizk_prove, &mut rng)
                .unwrap();
        end_timer!(groth_client_prove);
        let mut compressed_proof = Vec::new();
        groth_client_proof.serialize_compressed(&mut compressed_proof).unwrap();
        println!("groth_client_proof size (in bytes): {}", compressed_proof.len());

        let groth_client_verify = start_timer!(|| "Groth Client Verify");
        let mut groth_client_input = Vec::new();
        groth_client_input.append(
            &mut ToConstraintField::<ConstraintF>::to_field_elements(&request.x).unwrap(),
        );
        groth_client_input.append(
            &mut ToConstraintField::<ConstraintF>::to_field_elements(&request.y).unwrap(),
        );
        groth_client_input.append(
            &mut ToConstraintField::<ConstraintF>::to_field_elements(&client_id).unwrap(),
        );
        assert!(
            <Groth16<E2> as SNARK::<ConstraintF>>::verify(
                &groth_client_vk,
                &groth_client_input,
                &groth_client_proof
            )
            .is_ok(),
            "Groth Client Verification Failed"
        );
        end_timer!(groth_client_verify);
    }
}