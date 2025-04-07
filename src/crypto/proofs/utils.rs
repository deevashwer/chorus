use ark_ec::pairing::Pairing;
use ark_ff::PrimeField;
use ark_serialize::CanonicalSerialize;
use num_bigint::BigUint;
use rug::integer::Order;
use class_group::Integer;

pub fn bigint_to_integer(bigint: &BigUint) -> Integer {
    let bytes = bigint.to_bytes_le();
    Integer::from_digits(&bytes, Order::Lsf)
}

pub fn integer_to_bigint(integer: &Integer) -> BigUint {
    let bytes = integer.to_digits(Order::Lsf);
    BigUint::from_bytes_le(&bytes)
}

pub fn integer_to_field<F: PrimeField>(integer: &Integer) -> F {
    let bigint = integer_to_bigint(integer);
    F::from(bigint)
}

pub fn field_to_integer<F: PrimeField>(field: &F) -> Integer {
    let bigint: BigUint = field.into_bigint().into();
    bigint_to_integer(&bigint)
}

pub fn g1_affine_to_bytes<E: Pairing>(elem: &E::G1Affine) -> Vec<u8> {
    let mut bytes = Vec::new();
    elem.serialize_compressed(&mut bytes).unwrap();
    bytes
}

#[cfg(test)]
pub mod tests {
    #[test]
    pub fn test_int_converstions() {
        let bigint = num_bigint::BigUint::from(42u64);
        let integer = rug::Integer::from(42).into();
        let field = ark_bls12_377::Fr::from(42u64);
        let integer_from_bigint = super::bigint_to_integer(&bigint);
        let bigint_from_integer = super::integer_to_bigint(&integer);
        let field_from_integer = super::integer_to_field::<ark_bls12_377::Fr>(&integer);
        let integer_from_field = super::field_to_integer::<ark_bls12_377::Fr>(&field);
        assert_eq!(integer, integer_from_bigint);
        assert_eq!(bigint, bigint_from_integer);
        assert_eq!(field, field_from_integer);
        assert_eq!(integer, integer_from_field);
    }
}
