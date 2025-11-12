use core::num;
use std::cmp::Ordering;

use openssl::bn::{BigNum, BigNumContext};
use openssl::ec::EcGroup;
use openssl::nid::Nid;
use serde::{Serialize, Deserialize};
pub use vrf::openssl::Error as SortitionError;
use vrf::openssl::{CipherSuite, ECVRF};
use vrf::VRF;

pub struct SortitionState {
    vrf: ECVRF,
    group_order: BigNum,
}

#[derive(Clone)]
pub struct VRFSecretKey {
    sk: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VRFPublicKey {
    pub pk: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VRFProof {
    vrf_proof: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SortitionOutput {
    pub proof: Option<VRFProof>,
    pub success: bool,
}

impl SortitionOutput {
    pub fn bytesize(&self) -> usize {
        let mut output_size = 0;
        match &self.proof {
            Some(p) => {
                output_size += p.vrf_proof.len();
                output_size += 1;
            }
            None => {
                output_size += 1;
            }
        }
        output_size + 1
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        match &self.proof {
            Some(p) => {
                buf.push(1);
                buf.extend_from_slice(&p.vrf_proof);
            }
            None => {
                buf.push(0);
            }
        }
        buf.push(self.success as u8);
        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut offset = 0;
        let mut proof_option = bytes[offset];
        offset += 1;
        // Deserialize proof if it was not None
        let proof: Option<VRFProof> = if proof_option == 1 {
            const VRF_PROOF_LEN: usize = 81;
            let proof = VRFProof {
                vrf_proof: (&bytes[offset..offset + VRF_PROOF_LEN]).to_vec()
            };
            offset += VRF_PROOF_LEN;
            Some(proof)
        } else {
            None
        };
        // Deserialize success
        let success = bytes[offset] != 0;
        SortitionOutput {
            proof,
            success
        }
    }
}

impl SortitionState {
    pub fn new() -> Result<SortitionState, SortitionError> {
        let group = EcGroup::from_curve_name(Nid::SECP256K1)?;
        let mut bn_ctx = BigNumContext::new()?;
        let mut order = BigNum::new()?;
        let _ = group.order(&mut order, &mut bn_ctx);
        Ok(SortitionState {
            vrf: ECVRF::from_suite(CipherSuite::SECP256K1_SHA256_TAI).unwrap(),
            group_order: order,
        })
    }

    pub fn keygen(&mut self, client_id: usize) -> Result<(VRFSecretKey, VRFPublicKey), SortitionError> {
        let sk = if cfg!(feature = "deterministic") {
            let mut sk_bn = BigNum::new()?;
            let mut id_bn = BigNum::from_u32((client_id + 1) as u32)?;
            let mut bn_ctx = BigNumContext::new()?;
            let _ = sk_bn.nnmod(&mut id_bn, &self.group_order, &mut bn_ctx);
            sk_bn.to_vec()
        } else {
            let mut sk_bn = BigNum::new()?;
            let _ = self.group_order.rand_range(&mut sk_bn);
            sk_bn.to_vec()
        };
        let pk = self.vrf.derive_public_key(&sk)?;
        Ok((VRFSecretKey { sk }, VRFPublicKey { pk }))
    }

    fn check_hash(&self, hash: &Vec<u8>, num_clients: usize, committee_size: usize) -> Result<bool, SortitionError> {
        // if proof_hash * num_parties < committee_size * 2^{hashlen}, success
        let mut lhs = BigNum::from_slice(hash)?; // check error
        lhs.mul_word(num_clients as u32)?;
        let mut rhs = BigNum::from_u32(0)?;
        rhs.set_bit((hash.len() * 8) as i32)?;
        rhs.mul_word(committee_size as u32)?;
        Ok(lhs.cmp(&rhs) == Ordering::Less)
    }

    pub fn eval(
        &mut self,
        seed: &Vec<u8>,
        sk: &VRFSecretKey,
        num_clients: usize,
        committee_size: usize,
    ) -> Result<SortitionOutput, SortitionError> {
        let vrf_proof = self.vrf.prove(&sk.sk, seed).unwrap();
        let proof_hash = self.vrf.proof_to_hash(&vrf_proof).unwrap();
        match self.check_hash(&proof_hash, num_clients, committee_size) {
            Ok(true) => Ok(SortitionOutput {
                proof: Some(VRFProof { vrf_proof }),
                success: true,
            }),
            _ => Ok(SortitionOutput {
                proof: None,
                success: false,
            }),
        }
    }

    pub fn verify(
        &mut self,
        seed: &Vec<u8>,
        pk: &VRFPublicKey,
        output: &VRFProof,
        num_clients: usize,
        committee_size: usize,
    ) -> Result<bool, SortitionError> {
        let proof_hash = self.vrf.verify(&pk.pk, &output.vrf_proof, &seed)?;
        self.check_hash(&proof_hash, num_clients, committee_size)
    }
}

#[cfg(test)]
pub mod tests {
    use core::num;

    use super::*;

    #[test]
    fn test_correct_sortition() {
        let num_clients = 1000;
        let committee_size = 250;
        let seed: Vec<u8> = [0xFF; 32].to_vec();

        let mut num_selected = 0;
        for _ in 0..num_clients {
            let mut state = SortitionState::new().unwrap();
            let (sk, pk) = state.keygen(1).unwrap();
            let output = state.eval(&seed, &sk, num_clients, committee_size).unwrap();
            if output.success {
                let success = state.verify(&seed, &pk, &output.proof.unwrap(), num_clients, committee_size).unwrap();
                if success {
                    num_selected += 1;
                }
            }
        }
        println!(
            "Number of selected clients out of {}: {}",
            num_clients, num_selected
        );
    }
}
