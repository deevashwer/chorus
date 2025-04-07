use std::marker::PhantomData;

use ark_ec::pairing::Pairing;
use ark_ff::PrimeField;
use ark_std::rand::Rng;

pub struct ShamirSecretSharing<F: PrimeField> {
    pub _field: PhantomData<F>,
}

#[derive(Clone)]
pub struct Sharing<F: PrimeField> {
    pub shares: Vec<F>,
    pub coeffs: Vec<F>,
}

impl<F: PrimeField> ShamirSecretSharing<F> {
    pub fn new_sharing(
        secret: &F,
        num_shares: usize,
        threshold: usize,
        rng: &mut impl Rng,
    ) -> Sharing<F> {
        let mut coeffs = vec![secret.clone()];
        let random_coeffs: Vec<F> = (1..threshold).map(|_| F::rand(rng)).collect();
        coeffs.extend(random_coeffs);
        let shares: Vec<F> = (1..=num_shares)
            .map(|i| Self::poly_eval(&coeffs, &F::from(i as u64)))
            .collect();
        Sharing { shares, coeffs }
    }

    pub fn recover_secret(shares: &Vec<F>, pts: Option<&Vec<F>>) -> F {
        Self::lagrange_interpolation(shares, pts, 0)
    }

    pub fn lagrange_coeffs(pts: &Vec<F>, idx: usize) -> Vec<F> {
        let n = pts.len();
        let mut coeffs = vec![F::zero(); n];
        let idx_f = F::from(idx as u64);
        for i in 0..n {
            let mut term = F::one();
            for j in 0..n {
                if i != j {
                    term *= (idx_f - pts[j]) / (pts[i] - pts[j]);
                }
            }
            coeffs[i] = term;
        }
        coeffs
    }

    // let n = len(evals) and evals[i] = poly(i+1); returns evaluation at x = idx
    pub fn lagrange_interpolation(shares: &Vec<F>, pts: Option<&Vec<F>>, idx: usize) -> F {
        let n = shares.len();
        let pts = match pts {
            Some(pts) => {
                assert!(pts.len() == n);
                pts.clone()
            }
            None => {
                let pts: Vec<F> = (1..=n).map(|i| F::from(i as u64)).collect();
                pts
            }
        };
        let coeffs = Self::lagrange_coeffs(&pts, idx);
        let mut result = F::zero();
        for i in 0..n {
            result += coeffs[i] * shares[i];
        }
        result
    }

    // let n = len(evals) and evals[i] = poly(i+1); returns interpolated degree-(n-1) polynomial
    pub fn interpolate_poly(evals: &Vec<F>) -> Vec<F> {
        let n = evals.len();
        let mut coeffs = vec![F::zero(); n];
        // coeffs_L = \Prod_{j=1}^{n} (x - j))
        let mut coeffs_L = vec![F::one()];
        coeffs_L.extend(vec![F::zero(); n]);
        for j in 1..=n {
            let constant_term = -F::from(j as u64);
            for k in (1..=j).rev() {
                coeffs_L[k] = coeffs_L[k] + coeffs_L[k - 1];
                coeffs_L[k - 1] *= constant_term;
            }
        }
        for i in 0..n {
            let mut prod = F::one();
            // compute Prod_{j != i} (x_i - x_j)
            let i_f = F::from((i + 1) as u64);
            for j in 0..n {
                if j != i {
                    prod *= i_f - F::from((j + 1) as u64);
                }
            }
            // compute y_i/Prod_{j != i} (x_i - x_j)
            prod = evals[i as usize] / prod;

            // basis_poly = coeffs_L / (x - x_i)
            let mut basis_poly = vec![F::zero(); n];
            basis_poly[n - 1] = coeffs_L[n];
            let constant_term = -F::from((i + 1) as u64);
            for j in (1..n).rev() {
                basis_poly[j - 1] = coeffs_L[j] - constant_term * basis_poly[j];
            }

            // coeffs += term
            for j in 0..n {
                coeffs[j] += basis_poly[j] * prod;
            }
        }
        coeffs
    }

    // horner's method
    pub fn poly_eval(coeffs: &Vec<F>, x: &F) -> F {
        let mut result = F::zero();
        for coeff in coeffs.iter().rev() {
            result = result * x + coeff;
        }
        result
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use ark_bls12_377::Fr;
    use ark_ff::{UniformRand, Zero};

    #[test]
    fn test_shamir_secret_sharing() {
        let mut rng = ark_std::test_rng();
        let secret = Fr::rand(&mut rng);
        let num_shares = 100;
        let threshold = 50;
        let sharing =
            ShamirSecretSharing::<Fr>::new_sharing(&secret, num_shares, threshold, &mut rng);
        let recovered_secret = ShamirSecretSharing::<Fr>::recover_secret(&sharing.shares, None);
        assert_eq!(secret, recovered_secret);
        let coeffs = ShamirSecretSharing::<Fr>::interpolate_poly(&sharing.shares);
        for i in 0..threshold {
            assert_eq!(coeffs[i], sharing.coeffs[i]);
        }
        for i in threshold..num_shares {
            assert_eq!(coeffs[i], Fr::zero());
        }
    }
}
