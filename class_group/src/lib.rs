pub mod cpp_interface;
pub mod cl_hsm;
pub use cpp_interface::{CL_HSM, QFI, ClassGroup, Integer};
pub use cl_hsm::{SecretKey as CL_HSM_SecretKey, PublicKey as CL_HSM_PublicKey, Ciphertext as CL_HSM_Ciphertext, MRCiphertext as CL_HSM_MRCiphertext, EncryptionRandomness as CL_HSM_EncryptionRandomness};

#[cfg(test)]
pub mod tests {
    use rug::rand::RandState;
    use rand::rngs::OsRng;
    use crate::{cpp_interface::{CL_HSM, QFI, Integer}, CL_HSM_Ciphertext};

    #[test]
    fn test_bindings() {
        let q = Integer::from_str_radix("52435875175126190479447740508185965837690552500527637822603658699938581184513", 10).expect("Integer: from_str_radix failed").into();
        let seed = Integer::from_str_radix("42", 10).expect("Integer: from_str_radix failed");
        let cl_hsmqk = CL_HSM::new(&q, &seed, 128);
        println!("cl_hsmqk: {:?}", cl_hsmqk);
        let e = Integer::from_str_radix("5", 10).expect("Integer: from_str_radix failed");
        let h_e = cl_hsmqk.power_of_h(&e);
        println!("h_e: {:?}", h_e);
        let m = Integer::from_str_radix("7", 10).expect("Integer: from_str_radix failed");
        let f_m = cl_hsmqk.power_of_f(&m);
        println!("f_m: {:?}", f_m);
        let m_ = cl_hsmqk.dlog_in_F(&f_m);
        println!("m_: {:?}", m_);
        assert!(m_ == m);
    }

    #[test]
    fn test_cl_hsm() {
        let q = Integer::from_str_radix("52435875175126190479447740508185965837690552500527637822603658699938581184513", 10).expect("Integer: from_str_radix failed");
        let seed = Integer::from_str_radix("42", 10).expect("Integer: from_str_radix failed");
        let cl = CL_HSM::new(&q, &seed, 128);
        let mut os_rng = OsRng::default();
        let mut rng = RandState::new();
        let (sk, pk) = cl.keygen(&mut rng);
        println!("exponent_bound_ bits: {:?}", cl.exponent_bound_.significant_bits());
        println!("sk bits: {:?}", sk.sk_.significant_bits());
        println!("sk: {:?}", sk);
        println!("pk: {:?}", pk);
        let pk = cl.precompute_pk(&pk);
        println!("pk: {:?}", pk);
        let m = Integer::from_str_radix("133713371337", 10).expect("Integer: from_str_radix failed");
        let (ct, _) = cl.encrypt(&pk, &m);
        println!("ct: {:?}", ct);
        let m_ = cl.decrypt(&sk, &ct);
        assert!(m_ == m);
        println!("m_: {:?}", m_);

        // test serialization/de-serialization
        let ct_bytes = bincode::serialize(&ct).unwrap();
        let ct_: CL_HSM_Ciphertext = bincode::deserialize(&ct_bytes).unwrap();
        assert!(ct_ == ct);
    }
}