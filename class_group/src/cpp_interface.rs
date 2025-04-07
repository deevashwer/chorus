#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
extern crate gmp_mpfr_sys;
use rug::{integer::UnsignedPrimitive, Integer as RugInteger};
use core::ffi::c_long;
use std::{ffi::c_uint, fmt::{self, Formatter, Debug}, ops::Mul};
use serde::{Deserialize, Serialize, Deserializer};
use std::ops::{Deref, DerefMut, Add};

pub fn rug_integer_to_bytes(x: &RugInteger) -> Vec<u8> {
    let mut bytes: Vec<u8> = vec![];
    // from_digits only works for positive Integers, so store 0 or 1 depending on 
    // positive or negative and then call to_digits on absolute value
    if x.is_negative() {
        bytes.push(0);
    } else {
        bytes.push(1)
    }
    let x_bytes = x.clone().abs().to_digits(rug::integer::Order::Lsf);
    bytes.extend_from_slice(&x_bytes.len().to_le_bytes());
    bytes.extend_from_slice(&x_bytes);
    bytes
}

pub fn rug_integer_from_bytes(bytes: &[u8]) -> (RugInteger, usize) {
    let mut offset = 0;

    let x_sign = bytes[offset];
    offset += 1;

    let mut x_length_bytes = [0u8; 8];
    x_length_bytes.copy_from_slice(&bytes[offset..offset + 8]);
    let x_length = usize::from_le_bytes(x_length_bytes);
    offset += 8;

    let x = if x_sign == 1 {
        RugInteger::from_digits(&bytes[offset..offset + x_length], rug::integer::Order::Lsf)
    } else {
        RugInteger::from_digits(&bytes[offset..offset + x_length], rug::integer::Order::Lsf).mul(-1)
    };
    offset += x_length;
    (x, offset)
}

#[repr(C)]
#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Clone, Hash)]
pub struct Integer {
    pub x: RugInteger,
}

impl Integer {
    pub fn from_str_radix(s: &str, radix: i32) -> Result<Self, rug::integer::ParseIntegerError> {
        Ok(Integer { x: RugInteger::from_str_radix(s, radix)? })
    }

    pub fn from_digits<T: UnsignedPrimitive>(digits: &[T], order: rug::integer::Order) -> Self {
        Integer { x: RugInteger::from_digits(digits, order) }
    }

    pub fn from<R: Into<RugInteger>>(x: R) -> Self {
        Integer { x: x.into() }
    }
}

impl Mul for Integer {
    type Output = Integer;

    fn mul(self, rhs: Self) -> Self::Output {
        Integer { x: self.x.mul(rhs.x) }
    }
}

impl<'a, 'b> Mul<&'b Integer> for &'a Integer {
    type Output = Integer;

    fn mul(self, rhs: &Integer) -> Self::Output {
        Integer { x: (&self.x * &rhs.x).into() }
    }
}

impl Add for Integer {
    type Output = Integer;

    fn add(self, rhs: Self) -> Self::Output {
        Integer { x: self.x.add(rhs.x) }
    }
}

impl Deref for Integer {
    type Target = RugInteger;

    fn deref(&self) -> &Self::Target {
        &self.x
    }
}

impl DerefMut for Integer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.x
    }
}

impl From<RugInteger> for Integer {
    fn from(x: RugInteger) -> Self {
        Integer { x }
    }
}

impl Serialize for Integer {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: serde::Serializer {
        // Serialize as a byte array
        serializer.serialize_bytes(&rug_integer_to_bytes(&self.x))
    }
}

impl<'de> Deserialize<'de> for Integer {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserialize bytes
        let bytes: Vec<u8> = Deserialize::deserialize(deserializer)?;
        let (x, _) = rug_integer_from_bytes(&bytes);
        Ok(Integer { x })
    }
}

#[repr(C)]
#[derive(Debug, Default, PartialEq, Eq, Clone, Hash, Serialize, Deserialize)]
pub struct QFI {
    pub a_: Integer,
    pub b_: Integer,
    pub c_: Integer,
}

#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct ClassGroup {
      pub disc_: Integer,
      pub default_nucomp_bound_: Integer,
      pub class_number_bound_: Integer,
}

impl ClassGroup {
    pub fn one(&self) -> QFI {
        let r: QFI;
        unsafe {
            let ptr: &mut QFI = &mut (*ClassGroup_one(self));
            r = ptr.clone();
            QFI_delete(ptr);
        }
        r
    }

    pub fn nucomp(&self, f1: &QFI, f2: &QFI) -> QFI {
        let r: QFI;
        unsafe {
            let ptr: &mut QFI = &mut (*ClassGroup_nucomp(self, f1, f2));
            r = ptr.clone();
            QFI_delete(ptr);
        }
        r
    }
    
    pub fn nucompinv(&self, f1: &QFI, f2: &QFI) -> QFI {
        let r: QFI;
        unsafe {
            let ptr: &mut QFI = &mut (*ClassGroup_nucompinv(self, f1, f2));
            r = ptr.clone();
            QFI_delete(ptr);
        }
        r
    }

    pub fn nudupl(&self, f: &QFI) -> QFI {
        let r: QFI;
        unsafe {
            let ptr: &mut QFI = &mut (*ClassGroup_nudupl(self, f));
            r = ptr.clone();
            QFI_delete(ptr);
        }
        r
    }

    pub fn nudupl_n(&self, f: &QFI, niter: usize) -> QFI {
        let r: QFI;
        unsafe {
            let ptr: &mut QFI = &mut (*ClassGroup_nudupl_n(self, f, niter as c_long));
            r = ptr.clone();
            QFI_delete(ptr);
        }
        r
    }

    pub fn nupow(&self, f: &QFI, n: &Integer) -> QFI {
        let r: QFI;
        unsafe {
            let ptr: &mut QFI = &mut (*ClassGroup_nupow(self, f, n));
            r = ptr.clone();
            QFI_delete(ptr);
        }
        r
    }

    pub fn nupow_wprecomp(&self, f: &QFI, n: &Integer, d: usize, e: usize, fe: &QFI, fd: &QFI, fed: &QFI) -> QFI {
        let r: QFI;
        unsafe {
            let ptr: &mut QFI = &mut (*ClassGroup_nupow_wprecomp(self, f, n, d as c_long, e as c_long, fe, fd, fed));
            r = ptr.clone();
            QFI_delete(ptr);
        }
        r
    }
}

// CL_HSMqk
#[repr(C)]
#[derive(Debug, Default, Clone)]
pub struct CL_HSM {
      pub q_: Integer,
      pub k_: c_long,
      pub p_: Integer,
      pub M_: Integer,
      pub Cl_DeltaK_: ClassGroup,
      pub Cl_Delta_: ClassGroup,
      pub compact_variant_: bool,
      pub large_message_variant_: bool,
      pub h_: QFI,
      pub distance_: c_uint,
      pub exponent_bound_: Integer,
      pub d_: c_long,
      pub e_: c_long,
      pub h_e_precomp_: QFI,
      pub h_d_precomp_: QFI,
      pub h_de_precomp_: QFI,
}

impl CL_HSM {
    pub fn new(q: &Integer, seed: &Integer, seclevel: usize) -> Self {
        let cl_hsmqk: CL_HSM;
        unsafe {
            let ptr: &mut CL_HSM = &mut (*CL_HSMqk_init(q, seed, seclevel as c_uint));
            cl_hsmqk = ptr.clone();
            CL_HSMqk_delete(ptr);
        }
        cl_hsmqk
    }

    pub fn identity(&self) -> QFI {
        self.Cl_Delta_.one()
    }

    pub fn power_of_h(&self, e: &Integer) -> QFI {
        let h_e: QFI;
        unsafe {
            let ptr: &mut QFI = &mut (*CL_HSMqk_power_of_h(self, e));
            h_e = ptr.clone();
            QFI_delete(ptr);
        }
        h_e
    }

    pub fn power_of_f(&self, m: &Integer) -> QFI {
        let f_m: QFI;
        unsafe {
            let ptr: &mut QFI = &mut (*CL_HSMqk_power_of_f(self, m));
            f_m = ptr.clone();
            QFI_delete(ptr);
        }
        f_m
    }

    pub fn dlog_in_F(&self, fm: &QFI) -> Integer {
        let m_: Integer;
        unsafe {
            let ptr: &mut Integer = &mut (*CL_HSMqk_dlog_in_F(self, fm));
            m_ = ptr.clone();
            gmp_mpfr_sys::gmp::mpz_clear(ptr.x.as_raw_mut());
        }
        m_
    }
}

extern "C" {
    pub fn QFI_delete(f: *mut QFI);
    pub fn CL_HSMqk_delete(f: *mut CL_HSM);

    pub fn CL_HSMqk_init(q: *const Integer, seed: *const Integer, seclevel: c_uint) -> *mut CL_HSM;
    pub fn CL_HSMqk_power_of_h(C: *const CL_HSM, e: *const Integer) -> *mut QFI;
    pub fn CL_HSMqk_power_of_f(C: *const CL_HSM, m: *const Integer) -> *mut QFI;
    pub fn CL_HSMqk_dlog_in_F (C: *const CL_HSM, fm: *const QFI) -> *mut Integer;

    // identity element
    pub fn ClassGroup_one(G: *const ClassGroup) -> *mut QFI;
    // f1 + f2
    pub fn ClassGroup_nucomp(G: *const ClassGroup, f1: *const QFI, f2: *const QFI) -> *mut QFI;
    // f1 - f2
    pub fn ClassGroup_nucompinv(G: *const ClassGroup, f1: *const QFI, f2: *const QFI) -> *mut QFI;
    // 2 * f
    pub fn ClassGroup_nudupl(G: *const ClassGroup, f: *const QFI) -> *mut QFI;
    // 2^niter * f
    pub fn ClassGroup_nudupl_n(G: *const ClassGroup, f: *const QFI, niter: c_long) -> *mut QFI;
    // f^n
    pub fn ClassGroup_nupow(G: *const ClassGroup, f: *const QFI, n: *const Integer) -> *mut QFI;
    // f^n with some precomputation on f
    pub fn ClassGroup_nupow_wprecomp(G: *const ClassGroup, f: *const QFI, n: *const Integer, d: c_long, e: c_long, fe: *const QFI, fd: *const QFI, fed: *const QFI) -> *mut QFI;
}