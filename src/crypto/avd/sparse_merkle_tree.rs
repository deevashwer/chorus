use std::{
    collections::HashMap,
    error::Error as ErrorTrait,
    fmt,
    marker::PhantomData,
};
use rand::Rng;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::Digest;
use std::hash::Hash;
use crate::crypto::avd::error::{HashError, MerkleTreeError};

// Tips on optimizing implementation: https://ethresear.ch/t/optimizing-sparse-merkle-trees/3751/5

pub type MerkleDepth = u8;
pub type MerkleIndex = u64;
pub const MAX_DEPTH: u8 = 64;


//Note: Parameters must be chosen to allow for input length merging two fixed length outputs
pub trait FixedLengthCRH {
    const INPUT_SIZE_BITS: usize;

    type Output: Clone + Eq + core::fmt::Debug + Hash + Default + Serialize + DeserializeOwned;
    type Parameters: Clone + Default + Send + Sync;

    fn setup<R: Rng>(r: &mut R) -> Result<Self::Parameters, HashError>;
    fn evaluate(parameters: &Self::Parameters, input: &[u8]) -> Result<Self::Output, HashError>;
    fn evaluate_variable_length(parameters: &Self::Parameters, input: &[u8]) -> Result<Self::Output, HashError>;
    fn merge(parameters: &Self::Parameters, left: &Self::Output, right: &Self::Output) -> Result<Self::Output, HashError>;
}

// Implementation of CRH for a hasher derived from the Rust Crypto Digest trait

pub struct CRHFromDigest<D: Digest> {
    _digest: PhantomData<D>,
}

impl<D: Digest> FixedLengthCRH for CRHFromDigest<D> {
    const INPUT_SIZE_BITS: usize = 256; // D::output_size() is not const - requires 256 bit output
    type Output = [u8; 32];
    type Parameters = ();

    fn setup<R: Rng>(_r: &mut R) -> Result<Self::Parameters, HashError> {
        if <D as Digest>::output_size() != 32 {
            Err(HashError::GeneralError("incorrect output size".to_string()))
        } else {
            Ok(())
        }
    }

    fn evaluate(_parameters: &Self::Parameters, input: &[u8]) -> Result<Self::Output, HashError> {
        match D::digest(input).to_vec().try_into() {
            Ok(arr) => Ok(arr),
            Err(_) => Err(HashError::GeneralError("incorrect output size".to_string())),
        }
    }

    fn evaluate_variable_length(_parameters: &Self::Parameters, input: &[u8]) -> Result<Self::Output, HashError> {
        match D::digest(input).to_vec().try_into() {
            Ok(arr) => Ok(arr),
            Err(_) => Err(HashError::GeneralError("incorrect output size".to_string())),
        }
    }

    fn merge(_parameters: &Self::Parameters, left: &Self::Output, right: &Self::Output) -> Result<Self::Output, HashError> {
        let mut hasher = D::new();
        hasher.update(left.as_slice());
        hasher.update(right.as_slice());
        match hasher.finalize().to_vec().try_into() {
            Ok(arr) => Ok(arr),
            Err(_) => Err(HashError::GeneralError("incorrect output size".to_string())),
        }
    }
}

// TODO: Add const hash parameters
pub trait MerkleTreeParameters {
    const DEPTH: MerkleDepth;
    type H: FixedLengthCRH;

    fn is_valid() -> Result<bool, MerkleTreeError> {
        if Self::DEPTH < 1 || Self::DEPTH > MAX_DEPTH {
            return Err(MerkleTreeError::TreeDepth(Self::DEPTH));
        }
        Ok(true)
    }
}

pub struct SparseMerkleTree<P: MerkleTreeParameters> {
    tree: HashMap<(MerkleDepth, MerkleIndex), <P::H as FixedLengthCRH>::Output>,
    pub root: <P::H as FixedLengthCRH>::Output,
    sparse_initial_hashes: Vec<<P::H as FixedLengthCRH>::Output>,
    pub hash_parameters: <P::H as FixedLengthCRH>::Parameters,
    _parameters: PhantomData<P>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct MerkleTreePath<P: MerkleTreeParameters> {
    pub path: Vec<<P::H as FixedLengthCRH>::Output>,
    pub _parameters: PhantomData<P>,
}

impl<P: MerkleTreeParameters> Clone for MerkleTreePath<P> {
    fn clone(&self) -> Self {
        Self {
            path: self.path.clone(),
            _parameters: PhantomData,
        }
    }
}

impl<P: MerkleTreeParameters> Default for MerkleTreePath<P> {
    fn default() -> Self {
        Self {
            path: vec![<P::H as FixedLengthCRH>::Output::default(); P::DEPTH as usize],
            _parameters: PhantomData,
        }
    }
}

impl<P: MerkleTreeParameters> SparseMerkleTree<P> {
    pub fn new(
        initial_leaf_value: &[u8],
        hash_parameters: &<P::H as FixedLengthCRH>::Parameters,
    ) -> Result<Self, HashError> {
        // Compute initial hashes for each depth of tree
        let mut sparse_initial_hashes =
            vec![hash_leaf::<P::H>(&hash_parameters, initial_leaf_value)?];
        for i in 1..=(P::DEPTH as usize) {
            let child_hash = sparse_initial_hashes[i - 1].clone();
            sparse_initial_hashes.push(hash_inner_node::<P::H>(
                hash_parameters,
                &child_hash,
                &child_hash,
            )?);
        }
        sparse_initial_hashes.reverse();

        Ok(SparseMerkleTree {
            tree: HashMap::new(),
            root: sparse_initial_hashes[0].clone(),
            sparse_initial_hashes: sparse_initial_hashes,
            hash_parameters: hash_parameters.clone(),
            _parameters: PhantomData,
        })
    }

    pub fn update(&mut self, index: MerkleIndex, leaf_value: &[u8]) -> Result<(), MerkleTreeError> {
        if index >= 1_u64 << (P::DEPTH as u64) {
            return Err(MerkleTreeError::LeafIndex(index));
        }

        let mut i = index;
        self.tree.insert(
            (P::DEPTH, i),
            hash_leaf::<P::H>(&self.hash_parameters, leaf_value)?,
        );

        for d in (0..P::DEPTH).rev() {
            i >>= 1;
            let lc_i = i << 1;
            let rc_i = lc_i + 1;
            let lc_hash = match self.tree.get(&(d + 1, lc_i)) {
                Some(h) => h.clone(),
                None => self.sparse_initial_hashes[(d + 1) as usize].clone(),
            };
            let rc_hash = match self.tree.get(&(d + 1, rc_i)) {
                Some(h) => h.clone(),
                None => self.sparse_initial_hashes[(d + 1) as usize].clone(),
            };
            self.tree.insert(
                (d, i),
                hash_inner_node::<P::H>(&self.hash_parameters, &lc_hash, &rc_hash)?,
            );
        }
        self.root = self.tree.get(&(0, 0)).expect("root lookup failed").clone();
        Ok(())
    }

    pub fn lookup(&self, index: MerkleIndex) -> Result<MerkleTreePath<P>, MerkleTreeError> {
        if index >= 1_u64 << (P::DEPTH as u64) {
            return Err(MerkleTreeError::LeafIndex(index));
        }
        let mut path = Vec::new();

        let mut i = index;
        for d in (1..=P::DEPTH).rev() {
            let sibling_hash = match self.tree.get(&(d, i ^ 1)) {
                Some(h) => h.clone(),
                None => self.sparse_initial_hashes[d as usize].clone(),
            };
            path.push(sibling_hash);
            i >>= 1;
        }
        Ok(MerkleTreePath {
            path,
            _parameters: PhantomData,
        })
    }
}

impl<P: MerkleTreeParameters> MerkleTreePath<P> {
    pub fn compute_root(
        &self,
        leaf: &[u8],
        index: MerkleIndex,
        hash_parameters: &<P::H as FixedLengthCRH>::Parameters,
    ) -> Result<<P::H as FixedLengthCRH>::Output, MerkleTreeError> {
        if index >= 1_u64 << (P::DEPTH as u64) {
            return Err(MerkleTreeError::LeafIndex(index));
        }
        if self.path.len() != P::DEPTH as usize {
            return Err(MerkleTreeError::TreeDepth(self.path.len() as u8));
        }

        let mut i = index;
        let mut current_hash = hash_leaf::<P::H>(hash_parameters, leaf)?;
        for sibling_hash in self.path.iter() {
            current_hash = match i % 2 {
                0 => hash_inner_node::<P::H>(hash_parameters, &current_hash, sibling_hash)?,
                1 => hash_inner_node::<P::H>(hash_parameters, sibling_hash, &current_hash)?,
                _ => unreachable!(),
            };
            i >>= 1;
        }
        Ok(current_hash)
    }

    pub fn verify(
        &self,
        root: &<P::H as FixedLengthCRH>::Output,
        leaf: &[u8],
        index: MerkleIndex,
        hash_parameters: &<P::H as FixedLengthCRH>::Parameters,
    ) -> Result<bool, MerkleTreeError> {
        Ok(self.compute_root(leaf, index, hash_parameters)? == *root)
    }
}

pub fn hash_leaf<H: FixedLengthCRH>(
    parameters: &H::Parameters,
    leaf: &[u8],
) -> Result<H::Output, HashError> {
    H::evaluate_variable_length(parameters, leaf)
}

pub fn hash_inner_node<H: FixedLengthCRH>(
    parameters: &H::Parameters,
    left: &H::Output,
    right: &H::Output,
) -> Result<H::Output, HashError> {
    H::merge(&parameters, left, right)
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use bincode::de;
    use rand::{rngs::StdRng, SeedableRng};
    use ark_crypto_primitives::crh::{
        pedersen::{CRH, Window},
    };
    use sha2::Sha256;

    type H = CRHFromDigest<Sha256>;

    #[derive(Clone)]
    pub struct MerkleTreeTestParameters;

    impl MerkleTreeParameters for MerkleTreeTestParameters {
        const DEPTH: MerkleDepth = 63;
        type H = H;
    }

    pub struct MerkleTreeTinyTestParameters;

    impl MerkleTreeParameters for MerkleTreeTinyTestParameters {
        const DEPTH: MerkleDepth = 1;
        type H = H;
    }

    type TestMerkleTree = SparseMerkleTree<MerkleTreeTestParameters>;
    type TinyTestMerkleTree = SparseMerkleTree<MerkleTreeTinyTestParameters>;

    #[test]
    fn initialize_test() {
        let mut rng = StdRng::seed_from_u64(0u64);
        let crh_parameters = H::setup(&mut rng).unwrap();
        let tree = TinyTestMerkleTree::new(&[0u8; 16], &crh_parameters).unwrap();
        let leaf_hash = hash_leaf::<H>(&crh_parameters, &[0u8; 16]).unwrap();
        let root_hash = hash_inner_node::<H>(&crh_parameters, &leaf_hash, &leaf_hash).unwrap();
        assert_eq!(tree.root, root_hash);
    }

    #[test]
    fn update_and_verify_test() {
        let mut rng = StdRng::seed_from_u64(0u64);
        let crh_parameters = H::setup(&mut rng).unwrap();
        let mut tree = TestMerkleTree::new(&[0u8; 16], &crh_parameters).unwrap();
        let proof_0 = tree.lookup(0).unwrap();
        let proof_177 = tree.lookup(177).unwrap();
        let proof_255 = tree.lookup(255).unwrap();
        assert!(proof_0
            .verify(&tree.root, &[0u8; 16], 0, &crh_parameters)
            .unwrap());
        assert!(proof_177
            .verify(&tree.root, &[0u8; 16], 177, &crh_parameters)
            .unwrap());
        assert!(proof_255
            .verify(&tree.root, &[0u8; 16], 255, &crh_parameters)
            .unwrap());
        assert!(tree.update(177, &[1_u8; 16]).is_ok());
        assert!(proof_177
            .verify(&tree.root, &[1u8; 16], 177, &crh_parameters)
            .unwrap());
        assert!(!proof_177
            .verify(&tree.root, &[0u8; 16], 177, &crh_parameters)
            .unwrap());
        assert!(!proof_177
            .verify(&tree.root, &[1u8; 16], 0, &crh_parameters)
            .unwrap());
        assert!(!proof_0
            .verify(&tree.root, &[0u8; 16], 0, &crh_parameters)
            .unwrap());
        let updated_proof_0 = tree.lookup(0).unwrap();
        assert!(updated_proof_0
            .verify(&tree.root, &[0u8; 16], 0, &crh_parameters)
            .unwrap());

        // test serialization
        let serialized_proof = bincode::serialize(&proof_177).unwrap();
        let deserialized_proof: MerkleTreePath<MerkleTreeTestParameters> =
            bincode::deserialize(&serialized_proof).unwrap();
        assert!(deserialized_proof
            .verify(&tree.root, &[1u8; 16], 177, &crh_parameters)
            .unwrap());
        println!("Proof bytesize: {}", bincode::serialized_size(&proof_177).unwrap());
    }

    /*
    #[test]
    fn stress_test() {
        let mut rng = StdRng::seed_from_u64(0_u64);
        let start = Instant::now();
        let crh_parameters = H::setup(&mut rng).unwrap();
        let mut tree = TestMerkleTree::new(&[0u8; 16], &crh_parameters).unwrap();
        let tree_size = 10usize.pow(7);
        let mut kvs = vec![];
        for _ in 0..tree_size {
            kvs.push((rng.gen::<[u8; 32]>(), rng.gen::<[u8; 32]>()));
        }
        let end = start.elapsed().as_secs();
        println!("Setup time: {}s", end);
        let digest_0 = tree.root;

        let update_batch_size = 1000;
        let mut updates = vec![];
        for _ in 0..update_batch_size {
            updates.push((rng.gen::<[u8; 32]>(), rng.gen::<[u8; 32]>()));
        }
        let start = Instant::now();
        let (digest_1, proof) = avd.batch_update(&updates).unwrap();
        let end = start.elapsed().as_millis();
        println!("Batch Update time: {}ms", end);

        // test serialization
        let serialized_proof = bincode::serialize(&proof).unwrap();
        let deserialized_proof: UpdateProof<MerkleTreeAVDTestParameters> =
            bincode::deserialize(&serialized_proof).unwrap();
        println!("Serialized proof size: {} bytes", serialized_proof.len());

        let start = Instant::now();
        assert!(
            TestMerkleTreeAVD::verify_update(&crh_parameters, &digest_0, &digest_1, &deserialized_proof).unwrap()
        );
        let end = start.elapsed().as_millis();
        println!("Verify Update time: {}ms", end);
    }
    */
}