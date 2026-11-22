//! Float math for `no_std` (core Rust has no `sin`, `sqrt`... without an OS).
//!
//! `use neko_base::math::FloatExt;` gives `f32`/`f64` the usual methods,
//! computed by `libm` (soft-float targets such as the PS1 included). With
//! `std` the inherent methods are used instead, as method lookup prefers them.

pub trait FloatExt: Sized {
    fn sin(self) -> Self;
    fn cos(self) -> Self;
    fn tan(self) -> Self;
    fn asin(self) -> Self;
    fn acos(self) -> Self;
    fn atan(self) -> Self;
    fn atan2(self, other: Self) -> Self;
    fn sinh(self) -> Self;
    fn cosh(self) -> Self;
    fn tanh(self) -> Self;
    fn sin_cos(self) -> (Self, Self);
    fn sqrt(self) -> Self;
    fn powf(self, n: Self) -> Self;
    fn powi(self, n: i32) -> Self;
    fn exp(self) -> Self;
    fn ln(self) -> Self;
    fn log10(self) -> Self;
    fn log2(self) -> Self;
    fn floor(self) -> Self;
    fn ceil(self) -> Self;
    fn round(self) -> Self;
    fn trunc(self) -> Self;
    fn fract(self) -> Self;
    fn abs(self) -> Self;
    fn signum(self) -> Self;
    fn rem_euclid(self, rhs: Self) -> Self;
    fn hypot(self, other: Self) -> Self;
}

macro_rules! float_ext {
    ($t:ty, $sin:ident, $cos:ident, $tan:ident, $asin:ident, $acos:ident, $atan:ident, $atan2:ident, $sinh:ident, $cosh:ident, $tanh:ident,
     $sqrt:ident, $pow:ident, $exp:ident, $log:ident, $log10:ident, $log2:ident, $floor:ident, $ceil:ident, $round:ident, $trunc:ident,
     $fabs:ident, $fmod:ident, $hypot:ident) => {
        impl FloatExt for $t {
            fn sin(self) -> Self { libm::$sin(self) }
            fn cos(self) -> Self { libm::$cos(self) }
            fn tan(self) -> Self { libm::$tan(self) }
            fn asin(self) -> Self { libm::$asin(self) }
            fn acos(self) -> Self { libm::$acos(self) }
            fn atan(self) -> Self { libm::$atan(self) }
            fn atan2(self, other: Self) -> Self { libm::$atan2(self, other) }
            fn sinh(self) -> Self { libm::$sinh(self) }
            fn cosh(self) -> Self { libm::$cosh(self) }
            fn tanh(self) -> Self { libm::$tanh(self) }
            fn sin_cos(self) -> (Self, Self) { (libm::$sin(self), libm::$cos(self)) }
            fn sqrt(self) -> Self { libm::$sqrt(self) }
            fn powf(self, n: Self) -> Self { libm::$pow(self, n) }
            fn powi(self, n: i32) -> Self { libm::$pow(self, n as $t) }
            fn exp(self) -> Self { libm::$exp(self) }
            fn ln(self) -> Self { libm::$log(self) }
            fn log10(self) -> Self { libm::$log10(self) }
            fn log2(self) -> Self { libm::$log2(self) }
            fn floor(self) -> Self { libm::$floor(self) }
            fn ceil(self) -> Self { libm::$ceil(self) }
            fn round(self) -> Self { libm::$round(self) }
            fn trunc(self) -> Self { libm::$trunc(self) }
            fn fract(self) -> Self { self - libm::$trunc(self) }
            fn abs(self) -> Self { libm::$fabs(self) }
            fn signum(self) -> Self { if self.is_nan() { self } else if self.is_sign_negative() { -1.0 } else { 1.0 } }
            fn rem_euclid(self, rhs: Self) -> Self {
                let r = libm::$fmod(self, rhs);
                if r < 0.0 { r + libm::$fabs(rhs) } else { r }
            }
            fn hypot(self, other: Self) -> Self { libm::$hypot(self, other) }
        }
    };
}

float_ext!(f32, sinf, cosf, tanf, asinf, acosf, atanf, atan2f, sinhf, coshf, tanhf, sqrtf, powf, expf, logf, log10f, log2f, floorf, ceilf, roundf, truncf, fabsf, fmodf, hypotf);
float_ext!(f64, sin, cos, tan, asin, acos, atan, atan2, sinh, cosh, tanh, sqrt, pow, exp, log, log10, log2, floor, ceil, round, trunc, fabs, fmod, hypot);

#[cfg(test)]
mod tests {
    use super::FloatExt;

    #[test]
    fn libm_matches_std() {
        // Call through the trait explicitly (std's inherent methods would win).
        let x = 0.75f64;
        assert!((FloatExt::sin(x) - x.sin()).abs() < 1e-15);
        assert!((FloatExt::rem_euclid(-7.5f64, 2.0) - (-7.5f64).rem_euclid(2.0)).abs() < 1e-15);
        assert_eq!(FloatExt::round(2.5f32), 2.5f32.round());
        assert_eq!(FloatExt::signum(-0.0f64), (-0.0f64).signum());
    }
}
