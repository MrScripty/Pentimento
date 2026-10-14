//! Test-only consumer of the retained image/SIMD dependency contracts.

#[cfg(test)]
mod tests {
    use pulp::{Arch, Simd, WithSimd};

    // Both invocation spellings occur in the upstream crates. These assertions
    // also cover identifier transforms used in generated SIMD/dispatch names.
    pastey::paste! {
        fn [<macro_ SIMD:lower _value>]() -> u32 { 37 }
        struct [<codec:camel configuration:camel>];
    }

    pastey::item! {
        const [<CPU_ avx2:upper>]: u32 = 19;
    }

    #[test]
    fn maintained_macro_spelling_and_identifiers_work() {
        assert_eq!(macro_simd_value(), 37);
        assert_eq!(CPU_AVX2, 19);
        let _ = CodecConfiguration;
    }

    struct Scale<'a>(&'a mut [f64]);

    impl WithSimd for Scale<'_> {
        type Output = usize;

        fn with_simd<S: Simd>(self, simd: S) -> Self::Output {
            let lanes = size_of::<S::f64s>() / size_of::<f64>();
            let (head, tail) = S::as_mut_simd_f64s(self.0);
            for values in head {
                *values = simd.mul_f64s(simd.splat_f64s(3.0), *values);
            }
            for value in tail {
                *value *= 3.0;
            }
            lanes
        }
    }

    #[test]
    fn retained_pulp_generated_api_matches_scalar_arithmetic() {
        // Odd, non-vector-aligned lengths force vector and scalar-tail paths.
        for len in [0, 1, 3, 17, 257] {
            let input: Vec<_> = (0..len).map(|i| (i as f64 - 17.0) / 8.0).collect();
            let expected: Vec<_> = input.iter().map(|x| x * 3.0).collect();
            let mut dispatched = input.clone();
            let lanes = Arch::new().dispatch(Scale(&mut dispatched));
            assert_eq!(dispatched, expected);
            let mut scalar = input;
            assert_eq!(pulp::Scalar.vectorize(Scale(&mut scalar)), 1);
            assert_eq!(scalar, expected);
            #[cfg(target_arch = "x86_64")]
            assert!(lanes >= 2, "Linux x86_64 must exercise native SIMD");
        }
    }
}
