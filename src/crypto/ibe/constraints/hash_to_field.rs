use std::marker::PhantomData;

use ark_crypto_primitives::crh::sha256::constraints::Sha256Gadget;
use ark_ff::{Field, Fp2, Fp2Config, PrimeField};
use ark_r1cs_std::{alloc::AllocVar, fields::{fp::FpVar, fp2::Fp2Var, FieldVar}, uint8::UInt8, R1CSVar, ToConstraintFieldGadget};
use ark_relations::r1cs::SynthesisError;
use sha2::digest::DynDigest;

const MAX_DST_LENGTH: usize = 255;

const LONG_DST_PREFIX: [u8; 17] = [
    //'H', '2', 'C', '-', 'O', 'V', 'E', 'R', 'S', 'I', 'Z', 'E', '-', 'D', 'S', 'T', '-',
    0x48, 0x32, 0x43, 0x2d, 0x4f, 0x56, 0x45, 0x52, 0x53, 0x49, 0x5a, 0x45, 0x2d, 0x44, 0x53, 0x54,
    0x2d,
];

pub struct ExpanderXmdGadget<ConstraintF: PrimeField> {
    pub hasher: Sha256Gadget<ConstraintF>,
    pub dst: Vec<UInt8<ConstraintF>>,
    pub block_size: usize,
}

impl<ConstraintF: PrimeField> ExpanderXmdGadget<ConstraintF> {
    fn finalize_reset(hasher: Sha256Gadget<ConstraintF>) -> Result<(Vec<UInt8<ConstraintF>>, Sha256Gadget<ConstraintF>), SynthesisError> {
        let bytes_var = hasher.finalize()?.0;
        Ok((bytes_var, Sha256Gadget::default()))
    }

    fn construct_dst_prime(&self) -> Result<Vec<UInt8<ConstraintF>>, SynthesisError> {
        let mut dst_prime = if self.dst.len() > MAX_DST_LENGTH {
            let mut hasher = self.hasher.clone();
            let long_dst_prefix_var: Vec<UInt8<ConstraintF>> = UInt8::constant_vec(&LONG_DST_PREFIX);
            hasher.update(&long_dst_prefix_var)?;
            hasher.update(&self.dst)?;
            let (bytes_var, _) = Self::finalize_reset(hasher)?;
            bytes_var
        } else {
            self.dst.clone()
        };
        // TODO check: is it okay for this value to be a constant?
        dst_prime.push(UInt8::constant(dst_prime.len() as u8));
        Ok(dst_prime)
    }

    fn expand(&self, msg: &Vec<UInt8<ConstraintF>>, n: usize) -> Result<Vec<UInt8<ConstraintF>>, SynthesisError> {
        let mut hasher = self.hasher.clone();
        // output size of the hash function, e.g. 32 bytes = 256 bits for sha2::Sha256
        let b_len = 32;// hasher.output_size();
        let ell = (n + (b_len - 1)) / b_len;
        assert!(
            ell <= 255,
            "The ratio of desired output to the output size of hash function is too large!"
        );

        let dst_prime = self.construct_dst_prime()?;
        let z_pad: Vec<u8> = vec![0; self.block_size];
        let z_pad_var = UInt8::constant_vec(&z_pad);
        // // Represent `len_in_bytes` as a 2-byte array.
        // // As per I2OSP method outlined in https://tools.ietf.org/pdf/rfc8017.pdf,
        // // The program should abort if integer that we're trying to convert is too large.
        assert!(n < (1 << 16), "Length should be smaller than 2^16");
        let lib_str: [u8; 2] = (n as u16).to_be_bytes();
        let lib_str_var = UInt8::constant_vec(&lib_str);

        hasher.update(&z_pad_var)?;
        hasher.update(msg)?;
        hasher.update(&lib_str_var)?;
        hasher.update(&[UInt8::constant(0u8)])?;
        hasher.update(&dst_prime)?;
        let (b0, mut hasher) = Self::finalize_reset(hasher)?;

        hasher.update(&b0)?;
        hasher.update(&[UInt8::constant(1u8)])?;
        hasher.update(&dst_prime)?;
        let (mut bi, mut hasher) = Self::finalize_reset(hasher)?;

        let mut uniform_bytes: Vec<UInt8<ConstraintF>> = Vec::with_capacity(n);
        uniform_bytes.extend_from_slice(&bi);
        for i in 2..=ell {
            // update the hasher with xor of b_0 and b_i elements
            for (l, r) in b0.iter().zip(bi.iter()) {
                let xor: UInt8<ConstraintF> = l.xor(r)?;
                hasher.update(&[xor])?;
            }
            hasher.update(&[UInt8::constant(i as u8)])?;
            hasher.update(&dst_prime)?;
            (bi, hasher) = Self::finalize_reset(hasher)?;
            uniform_bytes.extend_from_slice(&bi);
        }
        Ok(uniform_bytes[0..n].to_vec())
    }
}

fn from_base_prime_field_elems<P: Fp2Config>(elems: &[FpVar<P::Fp>]) -> Option<Fp2Var<P>> {
    if elems.len() != (Fp2::<P>::extension_degree() as usize) {
        return None;
    }
    Some(Fp2Var::new(elems[0].clone(), elems[1].clone()))
}

pub struct HashToFp2Gadget<P: Fp2Config, const SEC_PARAM: usize = 128> {
    expander: ExpanderXmdGadget<P::Fp>,
    len_per_base_elem: usize,
}

impl<P: Fp2Config, const SEC_PARAM: usize, ConstraintF: PrimeField> HashToFp2Gadget<P, SEC_PARAM>
where P: Fp2Config<Fp = ConstraintF>
{
    pub fn new(dst: &Vec<UInt8<ConstraintF>>) -> Self {
        // The final output of `hash_to_field` will be an array of field
        // elements from F::BaseField, each of size `len_per_elem`.
        let len_per_base_elem = get_len_per_elem::<Fp2<P>, SEC_PARAM>();

        let expander = ExpanderXmdGadget {
            hasher: Sha256Gadget::default(),
            dst: dst.to_vec(),
            block_size: len_per_base_elem,
        };

        HashToFp2Gadget {
            expander,
            len_per_base_elem,
        }
    }

    pub fn hash_to_fp2(&self, message: &Vec<UInt8<ConstraintF>>, count: usize) -> Result<Vec<Fp2Var<P>>, SynthesisError> {
        let m = Fp2::<P>::extension_degree() as usize;

        // The user imposes a `count` of elements of F_p^m to output per input msg,
        // each field element comprising `m` BasePrimeField elements.
        let len_in_bytes = count * m * self.len_per_base_elem;
        let uniform_bytes = self.expander.expand(message, len_in_bytes)?;

        let mut output = Vec::with_capacity(count);
        let mut base_prime_field_elems = Vec::with_capacity(m);
        for i in 0..count {
            base_prime_field_elems.clear();
            for j in 0..m {
                let elm_offset = self.len_per_base_elem * (j + i * m);
                let val = from_be_bytes_mod_order(
                    &uniform_bytes[elm_offset..][..self.len_per_base_elem],
                )?;
                base_prime_field_elems.push(val);
            }
            let f = from_base_prime_field_elems(&base_prime_field_elems).unwrap();
            output.push(f);
        }

        Ok(output)
    }
}

/// Reads bytes in big-endian, and converts them to a field element.
/// If the integer represented by `bytes` is larger than the modulus `p`, this method
/// performs the appropriate reduction.
fn from_be_bytes_mod_order<ConstraintF: PrimeField>(bytes: &[UInt8<ConstraintF>]) -> Result<FpVar<ConstraintF>, SynthesisError> {
    let mut bytes_copy = bytes.to_vec();
    bytes_copy.reverse();
    from_le_bytes_mod_order(&bytes_copy)
}

/// Reads bytes in little-endian, and converts them to a field element.
/// If the integer represented by `bytes` is larger than the modulus `p`, this method
/// performs the appropriate reduction.
fn from_le_bytes_mod_order<ConstraintF: PrimeField>(bytes: &[UInt8<ConstraintF>]) -> Result<FpVar<ConstraintF>, SynthesisError> {
    let num_modulus_bytes = ((ConstraintF::MODULUS_BIT_SIZE + 7) / 8) as usize;
    let num_bytes_to_directly_convert = std::cmp::min(num_modulus_bytes - 1, bytes.len());
    // Copy the leading little-endian bytes directly into a field element.
    // The number of bytes directly converted must be less than the
    // number of bytes needed to represent the modulus, as we must begin
    // modular reduction once the data is of the same number of bytes as the
    // modulus.
    let (bytes, bytes_to_directly_convert) =
        bytes.split_at(bytes.len() - num_bytes_to_directly_convert);
    // Guaranteed to not be None, as the input is less than the modulus size.
    let mut res = bytes_to_directly_convert.to_constraint_field()?.pop().unwrap();

    // Update the result, byte by byte.
    // We go through existing field arithmetic, which handles the reduction.
    // TODO: If we need higher speeds, parse more bytes at once, or implement
    // modular multiplication by a u64
    let window_size = FpVar::<ConstraintF>::constant(256u64.into());
    for byte in bytes.iter().rev() {
        res *= &window_size;
        res += [byte.clone()].to_vec().to_constraint_field()?.pop().unwrap();
    }
    Ok(res)
}

/// This function computes the length in bytes that a hash function should output
/// for hashing an element of type `Field`.
/// See section 5.1 and 5.3 of the
/// [IETF hash standardization draft](https://datatracker.ietf.org/doc/draft-irtf-cfrg-hash-to-curve/14/)
fn get_len_per_elem<F: Field, const SEC_PARAM: usize>() -> usize {
    // ceil(log(p))
    let base_field_size_in_bits = F::BasePrimeField::MODULUS_BIT_SIZE as usize;
    // ceil(log(p)) + security_parameter
    let base_field_size_with_security_padding_in_bits = base_field_size_in_bits + SEC_PARAM;
    // ceil( (ceil(log(p)) + security_parameter) / 8)
    let bytes_per_base_field_elem =
        ((base_field_size_with_security_padding_in_bits + 7) / 8) as u64;
    bytes_per_base_field_elem as usize
}


#[cfg(test)]
pub mod tests {
    use ark_crypto_primitives::crh::sha256::constraints::Sha256Gadget;
    use ark_ec::hashing::HashToCurve;
    use ark_ff::field_hashers::DefaultFieldHasher;
    use ark_ff::field_hashers::HashToField;
    use ark_ff::Fp2;
    use ark_ff::Fp2Config;
    use ark_r1cs_std::fields::fp2::Fp2Var;
    use ark_r1cs_std::uint8::UInt8;
    use ark_r1cs_std::R1CSVar;
    use ark_relations::{ns, r1cs::ConstraintSystem};
    use sha2::Sha256;
    use sha2::digest::Digest;
    use ark_bls12_377::Fq as F;
    use ark_bls12_377::Fq2Config as P;
    use ark_std::rand::RngCore;

    use super::HashToFp2Gadget;

    #[test]
    pub fn test_sha256_constraints() {
        let mut rng = ark_std::test_rng();
        let cs = ConstraintSystem::<F>::new_ref();
        let mut sha256 = Sha256::default();
        let mut sha256_var = Sha256Gadget::default();

        println!("Number of constraints at start: {}", cs.num_constraints());
        // Append the same 7-byte string 20 times
        for i in 0..20 {
            let mut input_str = vec![0u8; 7];
            rng.fill_bytes(&mut input_str);

            let input_str_var = UInt8::new_witness_vec(cs.clone(), &input_str).unwrap();
            sha256_var.update(&input_str_var).unwrap();
            sha256.update(input_str);
            println!("Number of constraints after {}-th loop: {}", i, cs.num_constraints());
        }

        let output_str = sha256.finalize().to_vec();
        let output_str_var = sha256_var.finalize().unwrap().value().unwrap().to_vec();
        assert_eq!(
            output_str_var,
            output_str,
        );
        assert!(cs.is_satisfied().unwrap());
    }

    #[test]
    pub fn test_hash_to_fp2() {
        let cs = ConstraintSystem::<F>::new_ref();

        let prefix = b"CHORUS-BF-TIBE-H3";
        let prefix_var = UInt8::constant_vec(prefix);

        let input = b"test hash to fp2";
        let input_var = UInt8::new_witness_vec(cs.clone(), input).unwrap();

        let hasher = <DefaultFieldHasher<Sha256, 128> as HashToField<Fp2<P>>>::new(prefix);
        let output: Vec<Fp2<P>> = hasher.hash_to_field(input, 2);

        let hasher_var = HashToFp2Gadget::<P>::new(&prefix_var);
        let output_var: Vec<Fp2Var<P>> = hasher_var.hash_to_fp2(&input_var, 2).unwrap();
        println!("Number of constraints from hash_to_fp2: {}", cs.num_constraints());
        assert!(cs.is_satisfied().unwrap());

        assert_eq!(output_var.value().unwrap(), output);
    }
}
