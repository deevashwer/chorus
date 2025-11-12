pub mod error;
pub mod constraints;

use std::marker::PhantomData;
use ark_ec::hashing::map_to_curve_hasher::MapToCurve;
use ark_ec::{AffineRepr, CurveGroup};
use ark_ec::pairing::{Pairing, PairingOutput};
use ark_ec::hashing::{curve_maps::wb::{WBMap, WBConfig}, map_to_curve_hasher::MapToCurveBasedHasher, HashToCurve};
use ark_ff::Field;
use ark_std::rand::Rng;
use ark_ff::{field_hashers::{DefaultFieldHasher, HashToField}, PrimeField, UniformRand};
use ark_serialize::{CanonicalSerialize, CanonicalDeserialize, Compress};
use serde::{Deserialize, Serialize, de};
use sha2::{Digest, Sha256};
use std::ops::Mul;
use error::IBEError;

pub use ark_bls12_377::Bls12_377 as E;
pub type G1Affine = <E as Pairing>::G1Affine;
pub type G2Affine = <E as Pairing>::G2Affine;
pub type ScalarField = <E as Pairing>::ScalarField;

pub struct BonehFranklinIBE {
    _pairing: PhantomData<()>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct CipherText {
    u: G1Affine,
    v: Vec<u8>,
}

impl Serialize for CipherText {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut bytes = Vec::new();

        self.u.serialize_compressed(&mut bytes).unwrap();
        bytes.extend_from_slice(&bincode::serialize(&self.v).map_err(serde::ser::Error::custom)?);

        serializer.serialize_bytes(&bytes)
    }
}

impl<'de> Deserialize<'de> for CipherText {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        let mut cursor = std::io::Cursor::new(bytes);

        // Deserialize each field in order
        let u = G1Affine::deserialize_compressed_unchecked(&mut cursor).unwrap();
        let v: Vec<u8> = bincode::deserialize_from(&mut cursor).map_err(de::Error::custom)?;

        Ok(Self { u, v })
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PublicKey {
    pub pk: G1Affine,
}

impl Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut bytes = Vec::new();

        self.pk.serialize_compressed(&mut bytes).unwrap();

        serializer.serialize_bytes(&bytes)
    }
}

impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        let mut cursor = std::io::Cursor::new(bytes);

        // Deserialize each field in order
        let pk = G1Affine::deserialize_compressed_unchecked(&mut cursor).unwrap();

        Ok(Self { pk })
    }
}

pub struct MasterSecretKey {
    pub msk: ScalarField,
}

pub struct SecretKey {
    pub sk: G2Affine,
}

#[derive(Clone, PartialEq, Debug)]
pub struct BlindExtractionRequest {
    pub req: G2Affine,
}

impl Serialize for BlindExtractionRequest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut bytes = Vec::new();

        self.req.serialize_compressed(&mut bytes).unwrap();

        serializer.serialize_bytes(&bytes)
    }
}

impl<'de> Deserialize<'de> for BlindExtractionRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        let mut cursor = std::io::Cursor::new(bytes);

        // Deserialize each field in order
        let req = G2Affine::deserialize_compressed_unchecked(&mut cursor).unwrap();

        Ok(Self { req })
    }
}

pub struct Blind {
    pub blind: ScalarField,
}

impl Serialize for Blind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut bytes = Vec::new();

        self.blind.serialize_compressed(&mut bytes).unwrap();

        serializer.serialize_bytes(&bytes)
    }
}

impl<'de> Deserialize<'de> for Blind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        let mut cursor = std::io::Cursor::new(bytes);

        // Deserialize each field in order
        let blind = ScalarField::deserialize_compressed_unchecked(&mut cursor).unwrap();

        Ok(Self { blind })
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct BlindExtractionResponse {
    pub rsp: G2Affine,
}

impl Serialize for BlindExtractionResponse {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut bytes = Vec::new();

        self.rsp.serialize_compressed(&mut bytes).unwrap();

        serializer.serialize_bytes(&bytes)
    }
}

impl<'de> Deserialize<'de> for BlindExtractionResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        let mut cursor = std::io::Cursor::new(bytes);

        // Deserialize each field in order
        let rsp = G2Affine::deserialize_compressed_unchecked(&mut cursor).unwrap();

        Ok(Self { rsp })
    }
}

impl BonehFranklinIBE 
    // E::ScalarField: PrimeField, 
    // WBMap<<<E as Pairing>::G2 as CurveGroup>::Config>: MapToCurve<<E as Pairing>::G2>,
    // <<E as Pairing>::G2 as CurveGroup>::Config: WBConfig,
{
    pub fn keygen<R: Rng>(rng: &mut R) -> Result<(PublicKey, MasterSecretKey), IBEError> {
        let msk = ScalarField::rand(rng);
        let pk = G1Affine::generator().mul(msk);
        Ok((PublicKey{ pk: pk.into() }, MasterSecretKey { msk }))
    }

    pub fn extract(id: &Vec<u8>, msk: &MasterSecretKey) -> Result<SecretKey, IBEError> {
        let hashed_id: G2Affine = Self::h1(id)?;
        Ok(SecretKey { sk: hashed_id.mul(msk.msk).into() })
    }

    pub fn blind_extract_request<R: Rng>(id: &Vec<u8>, rng: &mut R) -> Result<(BlindExtractionRequest, Blind), IBEError> {
        let blind = ScalarField::rand(rng);
        let hashed_id = Self::h1(id)?;
        let req = hashed_id.mul(blind);
        Ok((BlindExtractionRequest { req: req.into() }, Blind { blind }))
    }

    pub fn blind_extract_response(request: &BlindExtractionRequest, msk: &MasterSecretKey) -> Result<BlindExtractionResponse, IBEError> {
        Ok(BlindExtractionResponse { rsp: request.req.mul(msk.msk).into() })
    }

    pub fn blind_extract(response: &BlindExtractionResponse, blind: &Blind) -> Result<SecretKey, IBEError> {
        let blind = match blind.blind.inverse() {
            Some(b) => b,
            None => return Err(IBEError::ExtractionError("inverse error".to_string())),
        };
        let sk: G2Affine = response.rsp.mul(blind).into();
        Ok(SecretKey { sk })
    }

    pub fn encrypt<R: Rng>(
        pk: &PublicKey,
        id: &Vec<u8>,
        msg: &Vec<u8>,
        rng: &mut R,
    ) -> Result<CipherText, IBEError> {
        assert!(msg.len() <= 32, "message length should be <= 32 bytes");
        let ell = 32;
        let mut msg = msg.clone();
        msg.extend(std::iter::repeat(0).take(32 - msg.len()));
        // choose random sigma of length ell
        let mut sigma = vec![0u8; ell];
        rng.fill_bytes(&mut sigma);
        // r = H3(pk, id, message, sigma)
        let r = Self::h3(pk, id, &msg, &sigma);
        // h_id = H1(id)
        let h_id: G2Affine = Self::h1(id)?;
        // t = pairing(pk, h_id)^r
        let t = E::pairing(pk.pk, h_id).mul(r);
        // c = (u, v)
        // u = g^r
        // v = (sigma || msg) XOR H2(t)
        let u = G1Affine::generator().mul(r);
        let hashed_pairing = Self::h2(&t, 2*ell);
        let v: Vec<u8> = sigma.iter().chain(msg.iter())
            .zip(hashed_pairing.iter())
            .map(|(&x1, &x2)| x1 ^ x2)
            .collect();
        Ok(CipherText{ u: u.into(), v })
    }

    pub fn decrypt(
        pk: &PublicKey,
        id: &Vec<u8>,
        sk: &SecretKey,
        c: &CipherText,
    ) -> Result<Vec<u8>, IBEError> { 
        let ell = 32;
        // t = pairing(c.u, sk.sk)
        let t = E::pairing(c.u, sk.sk);
        // msg_ = c.v XOR H2(t)
        let hashed_pairing = Self::h2(&t, 2*ell);
        let msg_: Vec<u8> = c.v.iter()
            .zip(hashed_pairing.iter())
            .map(|(&x1, &x2)| x1 ^ x2)
            .collect();
        // sigma = msg_[..ell], msg = msg_[ell..]
        let (sigma, msg) = msg_.split_at(ell);
        // r = H3(pk, id, msg, sigma)
        let r = Self::h3(pk, id, msg, &sigma);
        // if g^r != u, error
        if G1Affine::generator().mul(r) != c.u {
            return Err(IBEError::DecryptionError("decryption error".to_string()));
        }
        // else return m
        Ok(msg.to_vec())
    }
}

// helper functions for hash functions used for encryption
impl BonehFranklinIBE 
{
    pub fn h1(msg: &[u8]) -> Result<G2Affine, IBEError> {
        let hasher = MapToCurveBasedHasher::<
            <E as Pairing>::G2,
            DefaultFieldHasher<Sha256, 128>,
            WBMap<<<E as Pairing>::G2 as CurveGroup>::Config>,
        >::new(b"CHORUS-BF-TIBE-H1-G2")?;

        Ok(hasher.hash(msg)?)
    }

    fn h2(pairing_output: &PairingOutput<E>, output_len: usize) -> Vec<u8> {
        let mut serialized_bytes = Vec::new();
        pairing_output.serialize_with_mode(&mut serialized_bytes, Compress::Yes).unwrap();
        let mut result = Vec::with_capacity(output_len);
        let mut counter = 0u32;

        while result.len() < output_len {
            let mut hasher = Sha256::new_with_prefix(b"CHORUS-BF-TIBE-H2");
            hasher.update(&serialized_bytes);
            hasher.update(counter.to_le_bytes()); // Append the counter
            let hash = hasher.finalize();
            
            let remaining = output_len - result.len();
            result.extend_from_slice(&hash[..remaining.min(hash.len())]);

            counter += 1;
        }
        result
    }

    fn h3(pk: &PublicKey, id: &[u8], msg: &[u8], sigma: &[u8]) -> ScalarField {
        let mut hasher1 = Sha256::new_with_prefix(b"CHORUS-BF-TIBE-H3");
        hasher1.update(bincode::serialize(pk).unwrap());
        hasher1.update(id);
        hasher1.update(msg);
        hasher1.update(sigma);
        let hasher_res = hasher1.finalize();
        let hasher2 = <DefaultFieldHasher<Sha256, 128> as HashToField<ScalarField>>::new(b"CHORUS-BF-TIBE-H3");
        hasher2.hash_to_field(&hasher_res, 1).pop().unwrap()
    }
}

#[cfg(test)]
pub mod tests {
    use std::ffi::OsStr;

    use rand::{rngs::OsRng, Rng};

    use crate::crypto::ibe::{BlindExtractionRequest, BlindExtractionResponse, BonehFranklinIBE as BF, CipherText};
    type E = ark_bls12_377::Bls12_377;

    #[test]
    pub fn test_bf() {
        let mut rng = ark_std::test_rng();
        let (mpk, msk) = BF::keygen(&mut rng).unwrap();

        let id = b"alice@example.com".to_vec();
        let msg: [u8; 32] = OsRng.gen();
        let ctxt = BF::encrypt(&mpk, &id, &msg.to_vec(), &mut rng).unwrap();

        let ctxt_bytes = bincode::serialize(&ctxt).unwrap();
        let ctxt_: CipherText = bincode::deserialize(&ctxt_bytes).unwrap();
        assert_eq!(ctxt, ctxt_);

        let sk_id = BF::extract(&id, &msk).unwrap();
        let msg_ = BF::decrypt(&mpk, &id, &sk_id, &ctxt).unwrap();
        assert_eq!(msg.to_vec(), msg_);

        let (req, blind) = BF::blind_extract_request(&id, &mut rng).expect("Blind extract request failed");
        let req_bytes = bincode::serialize(&req).unwrap();
        let req_: BlindExtractionRequest = bincode::deserialize(&req_bytes).unwrap();
        assert_eq!(req, req_);

        let rsp = BF::blind_extract_response(&req, &msk).unwrap();
        let rsp_bytes = bincode::serialize(&rsp).unwrap();
        let rsp_: BlindExtractionResponse = bincode::deserialize(&rsp_bytes).unwrap();
        assert_eq!(rsp, rsp_);

        let sk_id_blind = BF::blind_extract(&rsp, &blind).unwrap();
        let msg_blind = BF::decrypt(&mpk, &id, &sk_id_blind, &ctxt).unwrap();
        assert_eq!(msg.to_vec(), msg_blind);
    }
}
