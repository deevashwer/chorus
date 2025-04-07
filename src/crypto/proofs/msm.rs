use class_group::{CL_HSM, QFI, Integer};
#[cfg(feature = "parallel")]
use rayon::prelude::*;

pub fn cl_msm_naive(cl: &CL_HSM, bases: &Vec<QFI>, scalars: &Vec<Integer>) -> QFI {
    let n = bases.len();
    assert!(bases.len() == scalars.len());
    let mut out: QFI = cl.identity();
    for i in 0..n {
        let out_i = cl.nupow(&bases[i], &scalars[i]);
        out = cl.nucomp(&out, &out_i);
    }
    out
}

/// The result of this function is only approximately `ln(a)`
/// [`Explanation of usage`]
///
/// [`Explanation of usage`]: https://github.com/scipr-lab/zexe/issues/79#issue-556220473
fn ln_without_floats(a: usize) -> usize {
    // log2(a) * ln(2)
    (ark_std::log2(a) * 69 / 100) as usize
}

/// Optimized implementation of multi-scalar multiplication (adapted from https://github.com/arkworks-rs/algebra/blob/master/ec/src/scalar_mul/variable_base/mod.rs)
pub fn cl_msm(cl: &CL_HSM, bases: &Vec<QFI>, scalars: &Vec<Integer>) -> QFI {
    let size = bases.len();
    assert!(bases.len() == scalars.len());
    let scalars_and_bases_iter = scalars.iter().zip(bases).filter(|(s, _)| !s.is_zero());

    let c = if size < 32 {
        3
    } else {
        ln_without_floats(size) + 2
    };

    let num_bits = scalars
        .iter()
        .map(|s| s.significant_bits())
        .max()
        .unwrap_or(0) as usize;
    let one = Integer::from(1);
    let zero: QFI = cl.identity();
    let window_starts: Vec<_> = (0..num_bits).step_by(c).collect();

    // Each window is of size `c`.
    // We divide up the bits 0..num_bits into windows of size `c`, and
    // in parallel process each such window.
    let window_sums: Vec<_> = window_starts.into_iter()
        .map(|w_start| {
            let mut res = zero.clone();
            // We don't need the "zero" bucket, so we only have 2^c - 1 buckets.
            let mut buckets = vec![zero.clone(); (1 << c) - 1];
            // This clone is cheap, because the iterator contains just a
            // pointer and an index into the original vectors.
            scalars_and_bases_iter.clone().for_each(|(scalar, base)| {
                let scalar = scalar.clone();
                if scalar == one {
                    // We only process unit scalars once in the first window.
                    if w_start == 0 {
                        // res += base;
                        res = cl.nucomp(&res, &base);
                    }
                } else {
                    let mut scalar = scalar;

                    // We right-shift by w_start, thus getting rid of the
                    // lower bits.
                    *scalar >>= w_start as u32;

                    // We mod the remaining bits by 2^{window size}, thus taking `c` bits.
                    // let scalar = scalar.as_ref()[0] % (1 << c);
                    let scalar = scalar.to_u64_wrapping() % (1 << c);

                    // If the scalar is non-zero, we update the corresponding
                    // bucket.
                    // (Recall that `buckets` doesn't have a zero bucket.)
                    if scalar != 0 {
                        let idx = (scalar - 1) as usize;
                        // buckets[idx] += base;
                        buckets[idx] = cl.nucomp(&buckets[idx], &base);
                    }
                }
            });

            // Compute sum_{i in 0..num_buckets} (sum_{j in i..num_buckets} bucket[j])
            // This is computed below for b buckets, using 2b curve additions.
            //
            // We could first normalize `buckets` and then use mixed-addition
            // here, but that's slower for the kinds of groups we care about
            // (Short Weierstrass curves and Twisted Edwards curves).
            // In the case of Short Weierstrass curves,
            // mixed addition saves ~4 field multiplications per addition.
            // However normalization (with the inversion batched) takes ~6
            // field multiplications per element,
            // hence batch normalization is a slowdown.

            // `running_sum` = sum_{j in i..num_buckets} bucket[j],
            // where we iterate backward from i = num_buckets to 0.
            let mut running_sum = cl.identity();
            buckets.into_iter().rev().for_each(|b| {
                // running_sum += &b;
                running_sum = cl.nucomp(&running_sum, &b);
                // res += &running_sum;
                res = cl.nucomp(&res, &running_sum);
            });
            res
        })
        .collect();

    // We store the sum for the lowest window.
    let lowest = *window_sums.first().as_ref().unwrap();

    // We're traversing windows from high to low.
    let rest = &window_sums[1..]
        .iter()
        .rev()
        .fold(zero.clone(), |mut total, sum_i| {
            total = cl.nucomp(&total, &sum_i);
            for _ in 0..c {
                total = cl.nudupl(&total);
            }
            total
        });
    cl.nucomp(&lowest, &rest)
}
