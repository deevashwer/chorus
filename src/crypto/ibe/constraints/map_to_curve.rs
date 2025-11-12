use std::{hash::Hash, marker::PhantomData, str::FromStr};

use ark_bls12_377::{Fq2Config, G2Affine};
use ark_ec::{bls12::Bls12Config, hashing::{curve_maps::{swu::SWUConfig, wb::WBConfig}, map_to_curve_hasher::MapToCurve, HashToCurve, HashToCurveError}, short_weierstrass::{Affine, Projective, SWCurveConfig}, AffineRepr, CurveConfig, CurveGroup, Group};
use ark_ff::{field_hashers::HashToField, BigInt, BigInteger, Field, Fp, Fp2, Fp2ConfigWrapper, LegendreSymbol, MontFp, One, PrimeField, SqrtPrecomputation, Zero};
use ark_r1cs_std::{fields::{fp::FpVar, fp2::Fp2Var, quadratic_extension::QuadExtVarConfig, FieldVar}, groups::{bls12::G2AffineVar, curves::short_weierstrass::{non_zero_affine::NonZeroAffineVar, AffineVar, ProjectiveVar}}, prelude::*};
use ark_relations::r1cs::SynthesisError;
use num_bigint::BigUint;
use sha2::digest::KeyInit;

use super::hash_to_field::HashToFp2Gadget;

/// Represents the SWU hash-to-curve map defined by `P`.
pub struct SWUMapGadget<P: Bls12Config> {
    _config: PhantomData<P>,
}

pub type IsoCurve<P> = <<P as Bls12Config>::G2Config as WBConfig>::IsogenousCurve;
pub type Fp2G<P> = Fp2Var<<P as Bls12Config>::Fp2Config>;

impl<P: Bls12Config> SWUMapGadget<P>
    where P::G2Config: WBConfig, IsoCurve<P>: SWUConfig,
{
    /// Constructs a new map if `P` represents a valid map.
    fn new() -> Result<Self, HashToCurveError> {
        // Verifying that ZETA is a non-square
        if IsoCurve::<P>::ZETA.legendre().is_qr() {
            return Err(HashToCurveError::MapToCurveError(
                "ZETA should be a quadratic non-residue for the SWU map".to_string(),
            ));
        }

        // Verifying the prerequisite for applicability  of SWU map
        if IsoCurve::<P>::COEFF_A.is_zero() || IsoCurve::<P>::COEFF_B.is_zero() {
            return Err(HashToCurveError::MapToCurveError("Simplified SWU requires a * b != 0 in the short Weierstrass form of y^2 = x^3 + a*x + b ".to_string()));
        }

        Ok(SWUMapGadget { _config: PhantomData })
    }

    /// Map an arbitrary base field element to a curve point.
    /// Based on
    /// <https://github.com/zcash/pasta_curves/blob/main/src/hashtocurve.rs>.
    fn map_to_curve(&self, point: &Fp2G<P>) -> Result<ProjectiveVar<IsoCurve::<P>, Fp2G<P>>, SynthesisError> {
        // 1. tv1 = inv0(Z^2 * u^4 + Z * u^2)
        // 2. x1 = (-B / A) * (1 + tv1)
        // 3. If tv1 == 0, set x1 = B / (Z * A)
        // 4. gx1 = x1^3 + A * x1 + B
        //
        // We use the "Avoiding inversions" optimization in [WB2019, section 4.2]
        // (not to be confused with section 4.3):
        //
        //   here       [WB2019]
        //   -------    ---------------------------------
        //   Z          ξ
        //   u          t
        //   Z * u^2    ξ * t^2 (called u, confusingly)
        //   x1         X_0(t)
        //   x2         X_1(t)
        //   gx1        g(X_0(t))
        //   gx2        g(X_1(t))
        //
        // Using the "here" names:
        //    x1 = num_x1/div      = [B*(Z^2 * u^4 + Z * u^2 + 1)] / [-A*(Z^2 * u^4 + Z * u^2]
        //   gx1 = num_gx1/div_gx1 = [num_x1^3 + A * num_x1 * div^2 + B * div^3] / div^3
        let a = Fp2G::<P>::constant(IsoCurve::<P>::COEFF_A);
        let b = Fp2G::<P>::constant(IsoCurve::<P>::COEFF_B);
        let zeta = Fp2G::<P>::constant(IsoCurve::<P>::ZETA);

        let zeta_u2 = &zeta * point.square()?;
        let ta = zeta_u2.square()? + &zeta_u2;
        let num_x1 = &b * (&ta + Fp2G::<P>::one());
        let ta_is_zero = ta.is_zero()?;
        let div = &a * ta_is_zero.select(&zeta, &ta.negate()?)?;

        let num2_x1 = num_x1.square()?;
        let div2 = div.square()?;
        let div3 = &div2 * &div;
        let num_gx1 = (&num2_x1 + (&a * &div2)) * &num_x1 + &b * &div3;

        // 5. x2 = Z * u^2 * x1
        let num_x2 = &zeta_u2 * &num_x1; // same div

        // 6. gx2 = x2^3 + A * x2 + B  [optimized out; see below]
        // 7. If is_square(gx1), set x = x1 and y = sqrt(gx1)
        // 8. Else set x = x2 and y = sqrt(gx2)
        let gx1 = &num_gx1 * &div3.inverse()?;
        let gx1_zeta = &zeta * &gx1;
        // let gx1_square = gx1.value().unwrap().legendre().is_qr();
        let gx1_square = match gx1.value() {
            Ok(gx1) => gx1.legendre().is_qr(),
            Err(_) => true,
        };
        let gx1_square_var = Boolean::new_witness(gx1.cs(), || Ok(gx1_square))?;
        let y1_value = if gx1_square {
            match gx1.value() {
                Ok(gx1) => Some(gx1.sqrt().unwrap()),
                Err(_) => None,
            }
        } else {
            match gx1_zeta.value() {
                Ok(gx1_zeta) => Some(gx1_zeta.sqrt().unwrap()),
                Err(_) => None,
            }
        };
        let y1 = Fp2G::<P>::new_witness(gx1.cs(), || y1_value.ok_or(SynthesisError::AssignmentMissing))?;
        let y1_square_expected = gx1_square_var.select(&gx1, &gx1_zeta)?;
        y1.square()?.enforce_equal(&y1_square_expected)?;

        // This magic also comes from a generalization of [WB2019, section 4.2].
        //
        // The Sarkar square root algorithm with input s gives us a square root of
        // h * s for free when s is not square, where h is a fixed nonsquare.
        // In our implementation, h = ROOT_OF_UNITY.
        // We know that Z / h is a square since both Z and h are
        // nonsquares. Precompute theta as a square root of Z / ROOT_OF_UNITY.
        //
        // We have gx2 = g(Z * u^2 * x1) = Z^3 * u^6 * gx1
        //                               = (Z * u^3)^2 * (Z/h * h * gx1)
        //                               = (Z * theta * u^3)^2 * (h * gx1)
        //
        // When gx1 is not square, y1 is a square root of h * gx1, and so Z * theta *
        // u^3 * y1 is a square root of gx2. Note that we don't actually need to
        // compute gx2.

        let y2 = zeta_u2 * point * &y1;
        let num_x = gx1_square_var.select(&num_x1, &num_x2)?;
        let y = gx1_square_var.select(&y1, &y2)?;
        // println!("y: {}", y.value().unwrap());

        let x_affine = num_x.mul_by_inverse(&div)?;
        let parity_check = Self::parity(&y)?.is_eq(&Self::parity(&point)?)?;
        // println!("parity_check: {}", parity_check.value().unwrap());
        let y_affine = parity_check.select(&y, &y.negate()?)?;
        let return_projective = ProjectiveVar::<IsoCurve::<P>, Fp2G<P>>::new(x_affine, y_affine, Fp2G::<P>::one());
        Ok(return_projective)
    }

    pub fn parity(element: &Fp2G<P>) -> Result<Boolean<P::Fp>, SynthesisError> {
        // if c0 nonzero, lsb of c0, else lsb of c1
        let c0_is_zero = element.c0.is_zero()?;
        let c0_bits = element.c0.to_bits_le()?;
        let c1_bits = element.c1.to_bits_le()?;
        let lsb_parity = c0_is_zero.select(c1_bits.first().unwrap(), c0_bits.first().unwrap())?;
        Ok(lsb_parity)
    }

    pub fn clear_cofactor(element: &ProjectiveVar<IsoCurve<P>, Fp2G<P>>) -> Result<AffineVar<IsoCurve<P>, Fp2G<P>>, SynthesisError> {
        let cofactor = IsoCurve::<P>::COFACTOR;
        let cofactor_bits = ark_ff::BitIteratorLE::without_trailing_zeros(cofactor);
        let cofactor_bits_var: Vec<Boolean<P::Fp>> = cofactor_bits.map(|b| Boolean::constant(b)).collect();
        element.scalar_mul_le(cofactor_bits_var.iter())?.to_affine()
    }
}

/*
// PSI_X = u^((p-1)/3)
const P_POWER_ENDOMORPHISM_COEFF_0 = Fp2::<P::Fp2Config>::new(
    MontFp!(
        "80949648264912719408558363140637477264845294720710499478137287262712535938301461879813459410946"
    ).into_repr(),
    P::Fp::ZERO,
);

// PSI_Y = u^((p-1)/2)
const P_POWER_ENDOMORPHISM_COEFF_1: ark_bls12_377::Fq2 = ark_bls12_377::Fq2::new(
    MontFp!(
        "216465761340224619389371505802605247630151569547285782856803747159100223055385581585702401816380679166954762214499"),
        ark_bls12_377::Fq::ZERO,
    );

// PSI_2_X = u^((p^2 - 1)/3)
const DOUBLE_P_POWER_ENDOMORPHISM_COEFF_0: ark_bls12_377::Fq2 = ark_bls12_377::Fq2::new(
        MontFp!("80949648264912719408558363140637477264845294720710499478137287262712535938301461879813459410945"),
        ark_bls12_377::Fq::ZERO
    );
*/

pub struct WBMapGadget<P: Bls12Config> {
    swu_field_curve_hasher: SWUMapGadget<P>,
    p_power_endomorphism_coeff_0: Fp2<P::Fp2Config>,
    p_power_endomorphism_coeff_1: Fp2<P::Fp2Config>,
    double_p_power_endomorphism_coeff_0: Fp2<P::Fp2Config>,
}

impl<P: Bls12Config> WBMapGadget<P> 
    where 
    P::G2Config: WBConfig {

    fn str_to_fp(s: &str) -> P::Fp {
        <P::Fp as PrimeField>::from_bigint(<P::Fp as PrimeField>::BigInt::try_from(BigUint::from_str(s).unwrap()).unwrap()).unwrap()
    }

    /// Constructs a new map if `P` represents a valid map.
    pub fn new() -> Result<Self, HashToCurveError> {

        // PSI_X = u^((p-1)/3)
        let p_power_endomorphism_coeff_0 = Fp2::<P::Fp2Config>::new(
            Self::str_to_fp("80949648264912719408558363140637477264845294720710499478137287262712535938301461879813459410946"),
            P::Fp::ZERO,
        );
        // PSI_Y = u^((p-1)/2)
        let p_power_endomorphism_coeff_1 = Fp2::<P::Fp2Config>::new(
            Self::str_to_fp("216465761340224619389371505802605247630151569547285782856803747159100223055385581585702401816380679166954762214499"),
            P::Fp::ZERO,
        );
        // PSI_2_X = u^((p^2 - 1)/3)
        let double_p_power_endomorphism_coeff_0 = Fp2::<P::Fp2Config>::new(
            Self::str_to_fp("80949648264912719408558363140637477264845294720710499478137287262712535938301461879813459410945"),
            P::Fp::ZERO,
        );
        Ok(WBMapGadget {
            swu_field_curve_hasher: SWUMapGadget::<P>::new().unwrap(),
            p_power_endomorphism_coeff_0,
            p_power_endomorphism_coeff_1,
            double_p_power_endomorphism_coeff_0,
        })
    }

    pub fn map_to_curve(&self, element: &Fp2G<P>) -> Result<ProjectiveVar<P::G2Config, Fp2G<P>>, SynthesisError> {
        // first we need to map the field point to the isogenous curve
        let point_on_isogenious_curve = self.swu_field_curve_hasher.map_to_curve(element)?;
        // let element_value = element.value().unwrap();
        // let swu_field_curve_hasher = &SWUMap::<IsoCurve<P>>::new().unwrap();
        // let point_on_isogenious_curve_value = swu_field_curve_hasher.map_to_curve(element_value).unwrap();
        // println!("point_on_isogenious_curve_value: {} {}", point_on_isogenious_curve_value.x, point_on_isogenious_curve_value.y);
        // println!("point_on_isogenious_curve_var: {} {}", point_on_isogenious_curve.x.value().unwrap(), point_on_isogenious_curve.y.value().unwrap());
        Self::apply(point_on_isogenious_curve)
    }

    pub fn apply(domain_point: ProjectiveVar<IsoCurve<P>, Fp2G<P>>) -> Result<ProjectiveVar<P::G2Config, Fp2G<P>>, SynthesisError> {
        let x_map_numerator_vars: Vec<Fp2G::<P>> = P::G2Config::ISOGENY_MAP.x_map_numerator.iter()
            .map(|elem| Fp2G::<P>::constant(*elem))
            .collect();
        let x_map_denominator_vars: Vec<Fp2G::<P>> = P::G2Config::ISOGENY_MAP.x_map_denominator.iter()
            .map(|elem| Fp2G::<P>::constant(*elem))
            .collect();
        let y_map_numerator_vars: Vec<Fp2G::<P>> = P::G2Config::ISOGENY_MAP.y_map_numerator.iter()
            .map(|elem| Fp2G::<P>::constant(*elem))
            .collect();
        let y_map_denominator_vars: Vec<Fp2G::<P>> = P::G2Config::ISOGENY_MAP.y_map_denominator.iter()
            .map(|elem| Fp2G::<P>::constant(*elem))
            .collect();

        let x_num = DensePolynomialVar::<P>::from_coefficients_vec(x_map_numerator_vars);
        let x_den = DensePolynomialVar::<P>::from_coefficients_vec(x_map_denominator_vars);

        let y_num = DensePolynomialVar::<P>::from_coefficients_vec(y_map_numerator_vars);
        let y_den = DensePolynomialVar::<P>::from_coefficients_vec(y_map_denominator_vars);

        let domain_point_affine = domain_point.to_affine()?;

        let v: [Fp2G::<P>; 2] = [x_den.evaluate(&domain_point_affine.x)?.inverse()?, y_den.evaluate(&domain_point_affine.x)?.inverse()?];
        let img_x = x_num.evaluate(&domain_point_affine.x)? * &v[0];
        let img_y = (y_num.evaluate(&domain_point_affine.x)? * &domain_point_affine.y) * &v[1];
        let projective_point = ProjectiveVar::<P::G2Config, Fp2G<P>>::new(img_x, img_y, Fp2G::<P>::one());
        Ok(projective_point)
    }

    /// psi(x,y) is the untwist-Frobenius-twist endomorhism on E'(Fq2)
    fn p_power_endomorphism(&self, p: &AffineVar<P::G2Config, Fp2G<P>>) -> Result<ProjectiveVar<P::G2Config, Fp2G<P>>, SynthesisError> {
        // The p-power endomorphism for G2 is defined as follows:
        // 1. Note that G2 is defined on curve E': y^2 = x^3 + 1/u.
        //    To map a point (x, y) in E' to (s, t) in E,
        //    one set s = x * (u ^ (1/3)), t = y * (u ^ (1/2)),
        //    because E: y^2 = x^3 + 1.
        // 2. Apply the Frobenius endomorphism (s, t) => (s', t'),
        //    another point on curve E, where s' = s^p, t' = t^p.
        // 3. Map the point from E back to E'; that is,
        //    one set x' = s' / ((u) ^ (1/3)), y' = t' / ((u) ^ (1/2)).
        //
        // To sum up, it maps
        // (x,y) -> (x^p * (u ^ ((p-1)/3)), y^p * (u ^ ((p-1)/2)))
        // as implemented in the code as follows.
        let p_power_endomorphism_coeff_0 = Fp2G::<P>::constant(self.p_power_endomorphism_coeff_0);
        let p_power_endomorphism_coeff_1 = Fp2G::<P>::constant(self.p_power_endomorphism_coeff_1);

        let mut res = p.clone();
        Fp2ConfigWrapper::<P::Fp2Config>::mul_base_field_var_by_frob_coeff(&mut res.x.c1, 1);
        Fp2ConfigWrapper::<P::Fp2Config>::mul_base_field_var_by_frob_coeff(&mut res.y.c1, 1);
        // res.x.frobenius_map_in_place(1);
        // res.y.frobenius_map_in_place(1);

        res.x *= p_power_endomorphism_coeff_0;
        res.y *= p_power_endomorphism_coeff_1;

        let res_projective = ProjectiveVar::<P::G2Config, Fp2G<P>>::new(res.x, res.y, Fp2G::<P>::one());
        Ok(res_projective)
    }

    /// For a p-power endomorphism psi(P), compute psi(psi(P))
    fn double_p_power_endomorphism(&self, p: &AffineVar<P::G2Config, Fp2G<P>>) -> Result<ProjectiveVar<P::G2Config, Fp2G<P>>, SynthesisError> {
        // p_power_endomorphism(&p_power_endomorphism(&p.into_affine())).into()
        let mut res = p.clone();

        let double_p_power_endomorphism_coeff_0 = Fp2G::<P>::constant(self.double_p_power_endomorphism_coeff_0);
        res.x *= double_p_power_endomorphism_coeff_0;
        // u^((p^2 - 1)/2) == -1
        res.y = res.y.negate()?;

        let res_projective = ProjectiveVar::<P::G2Config, Fp2G<P>>::new(res.x, res.y, Fp2G::<P>::one());
        Ok(res_projective)
    }

    pub fn clear_cofactor(&self, element: &ProjectiveVar<P::G2Config, Fp2G<P>>) -> Result<G2AffineVar<P>, SynthesisError> {
        // Based on Section 4.1 of https://eprint.iacr.org/2017/419.pdf
        // [h(ψ)]P = [x^2 − x − 1]P + [x − 1]ψ(P) + (ψ^2)(2P)
        let x: &'static [u64] = P::X;
        // println!("x: {:?}", x);
        let x_bits = ark_ff::BitIteratorLE::without_trailing_zeros(x);
        let x_bits_var: Vec<Boolean<P::Fp>> = x_bits.map(|b| Boolean::constant(b)).collect();

        let p = element.to_affine()?;

        // [x]P
        let x_p = element.scalar_mul_le(x_bits_var.iter())?;
        // let x_p_affine = x_p.to_affine()?;
        // let element_value = Projective::<P::G2Config>::new_unchecked(element.x.value().unwrap(), element.y.value().unwrap(), element.z.value().unwrap());
        // let x_p_value = element_value.mul_bigint(x);
        // println!("x_p: {} {}", x_p_affine.x.value().unwrap(), x_p_affine.y.value().unwrap());
        // println!("x_p_value: {}", x_p_value);
        // ψ(P)
        let psi_p = self.p_power_endomorphism(&p)?;
        // println!("psi_p: {} {}", psi_p.x.value().unwrap(), psi_p.y.value().unwrap());
        // (ψ^2)(2P)
        let mut psi2_p2 = self.double_p_power_endomorphism(&element.double()?.to_affine()?)?;
        // println!("psi2_p2: {} {}", psi2_p2.x.value().unwrap(), psi2_p2.y.value().unwrap());

        // tmp = [x]P + ψ(P)
        let mut tmp = x_p.clone();
        tmp += &psi_p;
        // let tmp_affine = tmp.to_affine()?;
        // println!("tmp: {} {}", tmp_affine.x.value().unwrap(), tmp_affine.y.value().unwrap());

        // tmp2 = [x^2]P + [x]ψ(P)
        let mut tmp2: ProjectiveVar<P::G2Config, Fp2G<P>> = tmp;
        tmp2 = tmp2.scalar_mul_le(x_bits_var.iter())?;
        // let tmp2_affine = tmp2.to_affine()?;
        // println!("tmp2: {} {}", tmp2_affine.x.value().unwrap(), tmp2_affine.y.value().unwrap());

        // add up all the terms
        psi2_p2 += tmp2;
        psi2_p2 -= x_p;
        psi2_p2 += &psi_p.negate()?;
        (psi2_p2 - element).to_affine()
    }
}

/// Stores a polynomial in coefficient form, where coeffcient is represented by
/// a list of `Fp2G<P>`.
/// adapted from https://docs.rs/ark-r1cs-std/latest/src/ark_r1cs_std/poly/polynomial/univariate/dense.rs.html
pub struct DensePolynomialVar<P: Bls12Config> {
        /// The coefficient of `x^i` is stored at location `i` in `self.coeffs`.
        pub coeffs: Vec<Fp2G<P>>,
    }
    
    impl<P: Bls12Config> DensePolynomialVar<P> {
        /// Constructs a new polynomial from a list of coefficients.
        pub fn from_coefficients_slice(coeffs: &[Fp2G<P>]) -> Self {
            Self::from_coefficients_vec(coeffs.to_vec())
        }
    
        /// Constructs a new polynomial from a list of coefficients.
        pub fn from_coefficients_vec(coeffs: Vec<Fp2G<P>>) -> Self {
            Self { coeffs }
        }
    
        /// Evaluates `self` at the given `point` and just gives you the gadget for
        /// the result. Caution for use in holographic lincheck: The output has
        /// 2 entries in one matrix
        pub fn evaluate(&self, point: &Fp2G<P>) -> Result<Fp2G<P>, SynthesisError> {
            let mut result = Fp2G::<P>::zero();
            // current power of point
            let mut curr_pow_x = Fp2G::<P>::one();
            for i in 0..self.coeffs.len() {
                let term = &curr_pow_x * &self.coeffs[i];
                result += &term;
                curr_pow_x *= point;
            }
    
            Ok(result)
         }
     }
pub struct MapToCurveBasedHasherGadget<P: Bls12Config>
{
    field_hasher: HashToFp2Gadget<P::Fp2Config>,
    curve_mapper: WBMapGadget<P>,
}

pub type ConstraintF<P> = <P as Bls12Config>::Fp;

impl<P: Bls12Config> MapToCurveBasedHasherGadget<P>
where P::G2Config: WBConfig, IsoCurve<P>: SWUConfig,
{
    pub fn new(domain: &Vec<UInt8<ConstraintF<P>>>) -> Result<Self, SynthesisError> {
        let field_hasher = HashToFp2Gadget::new(domain);
        let curve_mapper = WBMapGadget::new().unwrap();
        Ok(MapToCurveBasedHasherGadget {
            field_hasher,
            curve_mapper,
        })
    }

    // Produce a hash of the message, using the hash to field and map to curve
    // traits. This uses the IETF hash to curve's specification for Random
    // oracle encoding (hash_to_curve) defined by combining these components.
    // See https://tools.ietf.org/html/draft-irtf-cfrg-hash-to-curve-09#section-3
    pub fn hash(&self, msg: &Vec<UInt8<ConstraintF<P>>>) -> Result<G2AffineVar<P>, SynthesisError> {
        // IETF spec of hash_to_curve, from hash_to_field and map_to_curve
        // sub-components
        // 1. u = hash_to_field(msg, 2)
        // 2. Q0 = map_to_curve(u[0])
        // 3. Q1 = map_to_curve(u[1])
        // 4. R = Q0 + Q1              # Point addition
        // 5. P = clear_cofactor(R)
        // 6. return P

        let rand_field_elems = self.field_hasher.hash_to_fp2(msg, 2)?;
        // println!("rand_field_elems[0]_var: {}", rand_field_elems[0].value().unwrap());
        // println!("rand_field_elems[1]_var: {}", rand_field_elems[1].value().unwrap());

        let rand_curve_elem_0 = self.curve_mapper.map_to_curve(&rand_field_elems[0])?;
        let rand_curve_elem_1 = self.curve_mapper.map_to_curve(&rand_field_elems[1])?;
        // let rand_curve_elem_0_affine = rand_curve_elem_0.to_affine()?;
        // let rand_curve_elem_1_affine = rand_curve_elem_1.to_affine()?;
        // println!("rand_curve_elem_0_var: {} {}", rand_curve_elem_0_affine.x.value().unwrap(), rand_curve_elem_0_affine.y.value().unwrap());
        // println!("rand_curve_elem_1_var: {} {}", rand_curve_elem_1_affine.x.value().unwrap(), rand_curve_elem_1_affine.y.value().unwrap());

        let rand_curve_elem = rand_curve_elem_0 + rand_curve_elem_1;
        // let rand_curve_elem_affine = rand_curve_elem.to_affine()?;
        // println!("rand_curve_elem_var: {} {}", rand_curve_elem_affine.x.value().unwrap(), rand_curve_elem_affine.y.value().unwrap()); // this part doesn't match the non constraint version?
        let rand_subgroup_elem = self.curve_mapper.clear_cofactor(&rand_curve_elem)?;
        // println!("rand_subgroup_elem_var: {} {}", rand_subgroup_elem.x.value().unwrap(), rand_subgroup_elem.y.value().unwrap());

        Ok(rand_subgroup_elem)
    }
}

/// Represents the SWU hash-to-curve map defined by `P`.
pub struct SWUMap<P: SWUConfig>(PhantomData<fn() -> P>);

/// Trait defining a parity method on the Field elements based on [\[1\]] Section 4.1
///
/// - [\[1\]] <https://datatracker.ietf.org/doc/draft-irtf-cfrg-hash-to-curve/>
pub fn parity<F: Field>(element: &F) -> bool {
    element
        .to_base_prime_field_elements()
        .find(|&x| !x.is_zero())
        .map_or(false, |x| x.into_bigint().is_odd())
}

impl<P: SWUConfig> MapToCurve<Projective<P>> for SWUMap<P> {
    /// Constructs a new map if `P` represents a valid map.
    fn new() -> Result<Self, HashToCurveError> {
        // Verifying that ZETA is a non-square
        if P::ZETA.legendre().is_qr() {
            return Err(HashToCurveError::MapToCurveError(
                "ZETA should be a quadratic non-residue for the SWU map".to_string(),
            ));
        }

        // Verifying the prerequisite for applicability  of SWU map
        if P::COEFF_A.is_zero() || P::COEFF_B.is_zero() {
            return Err(HashToCurveError::MapToCurveError("Simplified SWU requires a * b != 0 in the short Weierstrass form of y^2 = x^3 + a*x + b ".to_string()));
        }

        Ok(SWUMap(PhantomData))
    }

    /// Map an arbitrary base field element to a curve point.
    /// Based on
    /// <https://github.com/zcash/pasta_curves/blob/main/src/hashtocurve.rs>.
    fn map_to_curve(&self, point: P::BaseField) -> Result<Affine<P>, HashToCurveError> {
        // 1. tv1 = inv0(Z^2 * u^4 + Z * u^2)
        // 2. x1 = (-B / A) * (1 + tv1)
        // 3. If tv1 == 0, set x1 = B / (Z * A)
        // 4. gx1 = x1^3 + A * x1 + B
        //
        // We use the "Avoiding inversions" optimization in [WB2019, section 4.2]
        // (not to be confused with section 4.3):
        //
        //   here       [WB2019]
        //   -------    ---------------------------------
        //   Z          ξ
        //   u          t
        //   Z * u^2    ξ * t^2 (called u, confusingly)
        //   x1         X_0(t)
        //   x2         X_1(t)
        //   gx1        g(X_0(t))
        //   gx2        g(X_1(t))
        //
        // Using the "here" names:
        //    x1 = num_x1/div      = [B*(Z^2 * u^4 + Z * u^2 + 1)] / [-A*(Z^2 * u^4 + Z * u^2]
        //   gx1 = num_gx1/div_gx1 = [num_x1^3 + A * num_x1 * div^2 + B * div^3] / div^3
        let a = P::COEFF_A;
        let b = P::COEFF_B;

        let zeta_u2 = P::ZETA * point.square();
        let ta = zeta_u2.square() + zeta_u2;
        let num_x1 = b * (ta + <P::BaseField as One>::one());
        let div = a * if ta.is_zero() { P::ZETA } else { -ta };

        let num2_x1 = num_x1.square();
        let div2 = div.square();
        let div3 = div2 * div;
        let num_gx1 = (num2_x1 + a * div2) * num_x1 + b * div3;

        // 5. x2 = Z * u^2 * x1
        let num_x2 = zeta_u2 * num_x1; // same div

        // 6. gx2 = x2^3 + A * x2 + B  [optimized out; see below]
        // 7. If is_square(gx1), set x = x1 and y = sqrt(gx1)
        // 8. Else set x = x2 and y = sqrt(gx2)
        let gx1_square;
        let gx1;

        assert!(
            !div3.is_zero(),
            "we have checked that neither a or ZETA are zero. Q.E.D."
        );
        let y1: P::BaseField = {
            gx1 = num_gx1 / div3;
            if gx1.legendre().is_qr() {
                gx1_square = true;
                gx1.sqrt()
                    .expect("We have checked that gx1 is a quadratic residue. Q.E.D")
            } else {
                let zeta_gx1 = P::ZETA * gx1;
                gx1_square = false;
                zeta_gx1.sqrt().expect(
                    "ZETA * gx1 is a quadratic residue because legard is multiplicative. Q.E.D",
                )
            }
        };

        // This magic also comes from a generalization of [WB2019, section 4.2].
        //
        // The Sarkar square root algorithm with input s gives us a square root of
        // h * s for free when s is not square, where h is a fixed nonsquare.
        // In our implementation, h = ROOT_OF_UNITY.
        // We know that Z / h is a square since both Z and h are
        // nonsquares. Precompute theta as a square root of Z / ROOT_OF_UNITY.
        //
        // We have gx2 = g(Z * u^2 * x1) = Z^3 * u^6 * gx1
        //                               = (Z * u^3)^2 * (Z/h * h * gx1)
        //                               = (Z * theta * u^3)^2 * (h * gx1)
        //
        // When gx1 is not square, y1 is a square root of h * gx1, and so Z * theta *
        // u^3 * y1 is a square root of gx2. Note that we don't actually need to
        // compute gx2.

        let y2 = zeta_u2 * point * y1;
        let num_x = if gx1_square { num_x1 } else { num_x2 };
        let y = if gx1_square { y1 } else { y2 };
        println!("y: {}", y);

        let x_affine = num_x / div;
        let y_affine = if parity(&y) != parity(&point) { -y } else { y };
        println!("parity_check: {}", parity(&y) == parity(&point));
        let point_on_curve = Affine::<P>::new_unchecked(x_affine, y_affine);
        assert!(
            point_on_curve.is_on_curve(),
            "swu mapped to a point off the curve"
        );
        Ok(point_on_curve)
    }
}

pub struct MapToCurveBasedHasher<T, H2F, M2C>
where
    T: CurveGroup,
    H2F: HashToField<T::BaseField>,
    M2C: MapToCurve<T>,
{
    field_hasher: H2F,
    curve_mapper: M2C,
    _params_t: PhantomData<T>,
}

impl<T, H2F, M2C> HashToCurve<T> for MapToCurveBasedHasher<T, H2F, M2C>
where
    T: CurveGroup,
    H2F: HashToField<T::BaseField>,
    M2C: MapToCurve<T>,
{
    fn new(domain: &[u8]) -> Result<Self, HashToCurveError> {
        let field_hasher = H2F::new(domain);
        let curve_mapper = M2C::new()?;
        let _params_t = PhantomData;
        Ok(MapToCurveBasedHasher {
            field_hasher,
            curve_mapper,
            _params_t,
        })
    }

    // Produce a hash of the message, using the hash to field and map to curve
    // traits. This uses the IETF hash to curve's specification for Random
    // oracle encoding (hash_to_curve) defined by combining these components.
    // See https://tools.ietf.org/html/draft-irtf-cfrg-hash-to-curve-09#section-3
    fn hash(&self, msg: &[u8]) -> Result<T::Affine, HashToCurveError> {
        // IETF spec of hash_to_curve, from hash_to_field and map_to_curve
        // sub-components
        // 1. u = hash_to_field(msg, 2)
        // 2. Q0 = map_to_curve(u[0])
        // 3. Q1 = map_to_curve(u[1])
        // 4. R = Q0 + Q1              # Point addition
        // 5. P = clear_cofactor(R)
        // 6. return P

        let rand_field_elems = self.field_hasher.hash_to_field(msg, 2);
        // println!("rand_field_elems[0]: {}", rand_field_elems[0]);
        // println!("rand_field_elems[1]: {}", rand_field_elems[1]);

        let rand_curve_elem_0 = self.curve_mapper.map_to_curve(rand_field_elems[0])?;
        let rand_curve_elem_1 = self.curve_mapper.map_to_curve(rand_field_elems[1])?;
        // println!("rand_curve_elem_0: {}", rand_curve_elem_0);
        // println!("rand_curve_elem_1: {}", rand_curve_elem_1);

        let rand_curve_elem = (rand_curve_elem_0 + rand_curve_elem_1).into();
        // println!("rand_curve_elem: {}", rand_curve_elem);
        let rand_subgroup_elem = rand_curve_elem.clear_cofactor();
        // println!("rand_subgroup_elem: {}", rand_subgroup_elem);

        Ok(rand_subgroup_elem)
    }
}


#[cfg(test)]
pub mod tests {
    use super::*;
    use ark_ec::{hashing::{curve_maps::{swu::SWUMap, wb::WBMap}, HashToCurve}, pairing::Pairing, short_weierstrass::Affine, AffineRepr, CurveGroup};
    use ark_bls12_377::{Config as P, Fq, Bls12_377 as E};
    use ark_ff::{field_hashers::DefaultFieldHasher, Fp2, UniformRand};
    use ark_relations::{ns, r1cs::ConstraintSystem};
    use rand::Rng;
    use sha2::Sha256;

    /*
    pub fn test_parity() {
        let mut rng = ark_std::test_rng();
        let cs = ConstraintSystem::new_ref();
        let c0 = Fq::from(rng.gen::<u128>());
        let c1 = Fq::from(rng.gen::<u128>());
        let field_elem = Fq2::new(c0, c1);
        let c0_var = FpVar::<Fq>::new_input(ark_relations::ns!(cs, "c0_var"), || Ok(c0)).unwrap();
        let c1_var = FpVar::<Fq>::new_input(ark_relations::ns!(cs, "c1_var"), || Ok(c1)).unwrap();
        let field_elem_var = Fp2Var::<Fq2Config>::new(c0_var, c1_var);
    }
    */

    #[test]
    pub fn test_swu_map_to_curve() {
        // random Fp2 and equivalent Fp2Var
        let mut rng = ark_std::test_rng();
        let cs = ConstraintSystem::<Fq>::new_ref();

        let x: Fp2<<P as Bls12Config>::Fp2Config> = Fp2::rand(&mut rng);
        let swu_map = SWUMap::<<<P as Bls12Config>::G2Config as WBConfig>::IsogenousCurve>::new().unwrap();
        // let y: Affine<<P as Bls12Config>::G2Config> = swu_map.map_to_curve(x).unwrap().try_into().unwrap();
        let y = swu_map.map_to_curve(x).unwrap();
        let y_subgroup = y.clear_cofactor();
        assert!(y_subgroup.is_on_curve());
        assert!(y_subgroup.is_in_correct_subgroup_assuming_on_curve());

        let x_var = Fp2Var::<Fq2Config>::new_witness(cs.clone(), || Ok(x)).unwrap();
        let swu_map_gadget = SWUMapGadget::<P>::new().unwrap();
        // this will be mapped to the curve, but not to the correct subgroup, need to clear the cofactor for that
        let y_var = swu_map_gadget.map_to_curve(&x_var).unwrap();
        let y_subgroup_var = SWUMapGadget::<P>::clear_cofactor(&y_var).unwrap();

        assert!(cs.is_satisfied().unwrap());
        println!("Number of constraints from map_to_curve_G2: {}", cs.num_constraints());
        // the following check will fail because there's more we need to do before ending up in the right subgroup
        assert_eq!(y_subgroup_var.value().unwrap(), y_subgroup);

        println!("y: {}", y);
        let y_affine_var = y_var.to_affine().unwrap();
        println!("y_var: {} {}", y_affine_var.x.value().unwrap(), y_affine_var.y.value().unwrap());
    }

    #[test]
    pub fn test_map_to_curve_based_hasher() {
        let cs = ConstraintSystem::<Fq>::new_ref();

        let prefix = b"CHORUS-BF-TIBE-H1-G2";
        let prefix_var = UInt8::constant_vec(prefix);

        let input = b"testing hash to G2";
        let input_var = UInt8::new_witness_vec(cs.clone(), input).unwrap();

        let hasher = MapToCurveBasedHasher::<
            <E as Pairing>::G2,
            DefaultFieldHasher<Sha256, 128>,
            WBMap<<<E as Pairing>::G2 as CurveGroup>::Config>>::new(prefix).unwrap();
        let hashed = hasher.hash(input).unwrap();

        let hasher_var = MapToCurveBasedHasherGadget::<P>::new(&prefix_var).unwrap();
        let hashed_var = hasher_var.hash(&input_var).unwrap();

        println!("Number of constraints from map_to_curve_based_hasher: {}", cs.num_constraints());

        println!("hashed: {}", hashed);
        println!("hashed_var: {}", hashed_var.value().unwrap());
        assert!(cs.is_satisfied().unwrap());
        assert_eq!(hashed_var.value().unwrap(), hashed);
    }
}

