use num_traits::ToPrimitive;

use crate::cmyk_to_srgb::CMYK_TO_SRGB;

/// Unpremultiplied color with RGBA channel
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export_to = "annotation_contract.ts"))]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    /// Fully transparent black, which paints nothing.
    pub const TRANSPARENT: Self = Self::from_rgba(0.0, 0.0, 0.0, 0.0);

    /// Returs color value from rgba component values.
    ///
    /// # Arguments
    ///
    /// - 'r': Value of red channel. the value needs between [0.0, 1.0]
    /// - 'g': Value of green channel. the value needs between [0.0, 1.0]
    /// - 'b': Value of blue channel. the value needs between [0.0, 1.0]
    /// - 'a': Value of alpha channel, clamped to [0.0, 1.0]
    pub const fn from_rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self::from_rgb(r, g, b).with_alpha(a)
    }

    pub const fn from_rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Returns the same color channels with alpha replaced by `a`, clamped to [0.0, 1.0].
    pub const fn with_alpha(self, a: f32) -> Self {
        Self {
            a: a.clamp(0.0, 1.0),
            ..self
        }
    }

    /// Returns the same color channels at full opacity.
    pub const fn opaque(self) -> Self {
        self.with_alpha(1.0)
    }

    /// Returns whether every channel, alpha included, is finite and within `[0.0, 1.0]`.
    pub fn is_valid(&self) -> bool {
        [self.r, self.g, self.b, self.a]
            .iter()
            .all(|value| (0.0..=1.0).contains(value))
    }

    /// Converts the color to 8-bit RGBA channel values.
    ///
    /// Each channel is clamped to `[0.0, 1.0]`, scaled to `[0, 255]`, and
    /// rounded to the nearest integer.
    pub fn to_rgba8(self) -> [u8; 4] {
        [
            float_channel_to_u8(self.r),
            float_channel_to_u8(self.g),
            float_channel_to_u8(self.b),
            float_channel_to_u8(self.a),
        ]
    }

    /// Returns a grayscale color from a single luminance value.
    ///
    /// # Arguments
    ///
    /// - `gray`: The gray level, a value between 0.0 (black) and 1.0 (white).
    ///
    /// Alpha defaults to 1.0 (opaque). This does not clamp the input; callers
    /// should ensure the value is within the valid range.
    pub const fn from_gray(gray: f32) -> Self {
        Self {
            r: gray,
            g: gray,
            b: gray,
            a: 1.0,
        }
    }

    /// Returns color value from CMYK component values.
    ///
    /// Component values are clamped to [0.0, 1.0]. Instead of the naive
    /// `(1 - c) * (1 - k)` inversion, which renders press colors too bright and
    /// saturated, the conversion approximates the Adobe-style CMYK to sRGB
    /// conversion that viewers apply to uncalibrated DeviceCMYK, using a
    /// fourth-order polynomial in the four components.
    /// Alpha defaults to 1.0 (opaque).
    pub fn from_cmyk(c: f32, m: f32, y: f32, k: f32) -> Self {
        let [c, m, y, k] = [c, m, y, k].map(|component| component.clamp(0.0, 1.0));
        // Products of two components, from which the higher degrees are built.
        let [cc, cm, cy, ck, mm, my, mk, yy, yk, kk] = [
            c * c,
            c * m,
            c * y,
            c * k,
            m * m,
            m * y,
            m * k,
            y * y,
            y * k,
            k * k,
        ];
        // Monomials in the order the coefficient rows expect.
        let terms = [
            c,
            m,
            y,
            k,
            cc,
            cm,
            cy,
            ck,
            mm,
            my,
            mk,
            yy,
            yk,
            kk,
            cc * c,
            cc * m,
            cc * y,
            cc * k,
            cm * m,
            cm * y,
            cm * k,
            cy * y,
            cy * k,
            ck * k,
            mm * m,
            mm * y,
            mm * k,
            my * y,
            my * k,
            mk * k,
            yy * y,
            yy * k,
            yk * k,
            kk * k,
            cc * cc,
            cc * cm,
            cc * cy,
            cc * ck,
            cc * mm,
            cc * my,
            cc * mk,
            cc * yy,
            cc * yk,
            cc * kk,
            cm * mm,
            cm * my,
            cm * mk,
            cm * yy,
            cm * yk,
            cm * kk,
            cy * yy,
            cy * yk,
            cy * kk,
            ck * kk,
            mm * mm,
            mm * my,
            mm * mk,
            mm * yy,
            mm * yk,
            mm * kk,
            my * yy,
            my * yk,
            my * kk,
            mk * kk,
            yy * yy,
            yy * yk,
            yy * kk,
            yk * kk,
            kk * kk,
        ];
        let [r, g, b] = CMYK_TO_SRGB.map(|coefficients| {
            let offset: f32 = coefficients
                .iter()
                .zip(terms)
                .map(|(coefficient, term)| coefficient * term)
                .sum();
            ((255.0 + offset) / 255.0).clamp(0.0, 1.0)
        });
        Self { r, g, b, a: 1.0 }
    }

    /// Creates a color from device color-space components.
    ///
    /// The component count determines the device color space: one component
    /// is grayscale, three components are RGB, and four components are CMYK.
    /// Components are clamped to `[0.0, 1.0]`, the nearest valid device value.
    /// Returns `None` for any other component count.
    #[must_use]
    pub fn from_device_components(components: &[f32]) -> Option<Self> {
        let unit = |component: f32| component.clamp(0.0, 1.0);
        match *components {
            [gray] => Some(Self::from_gray(unit(gray))),
            [r, g, b] => Some(Self::from_rgb(unit(r), unit(g), unit(b))),
            [c, m, y, k] => Some(Self::from_cmyk(c, m, y, k)),
            _ => None,
        }
    }

    /// Converts a CIE L\*a\*b\* color to sRGB.
    ///
    /// Implements the standard pipeline: LAB → XYZ (using the PDF-provided white point)
    /// → linear sRGB (IEC 61966-2-1 D65 matrix) → gamma-encoded sRGB.
    ///
    /// # Arguments
    ///
    /// - `l`: Lightness component, typically in the range [0, 100].
    /// - `a`: Green–red axis, typically in the range [-128, 127].
    /// - `b`: Blue–yellow axis, typically in the range [-128, 127].
    /// - `white_point`: Reference white in XYZ [Xw, Yw, Zw] (e.g., D65: [0.9505, 1.0, 1.089]).
    ///
    /// Output channels are clamped to [0.0, 1.0]. Alpha defaults to 1.0 (opaque).
    pub fn from_lab(l: f32, a: f32, b: f32, white_point: [f32; 3]) -> Self {
        // LAB → XYZ using the inverse CIE f function.
        let delta: f32 = 6.0 / 29.0;
        let delta_sq = delta * delta;

        let f_inv = |t: f32| -> f32 {
            if t > delta {
                t * t * t
            } else {
                3.0 * delta_sq * (t - 4.0 / 29.0)
            }
        };

        let f_y = (l + 16.0) / 116.0;
        let f_x = a / 500.0 + f_y;
        let f_z = f_y - b / 200.0;

        let [xw, yw, zw] = white_point;
        let x = xw * f_inv(f_x);
        let y = yw * f_inv(f_y);
        let z = zw * f_inv(f_z);

        // XYZ → linear sRGB (IEC 61966-2-1 D65 matrix).
        let r_lin = 3.240_454_2 * x - 1.537_138_5 * y - 0.498_531_4 * z;
        let g_lin = -0.969_266 * x + 1.876_010_8 * y + 0.041_556 * z;
        let b_lin = 0.055_643_4 * x - 0.204_025_9 * y + 1.057_225_2 * z;

        // Linear sRGB → gamma-encoded sRGB, clamped to [0, 1].
        let gamma = |c: f32| -> f32 {
            let c = c.clamp(0.0, 1.0);
            if c <= 0.003_130_8 {
                12.92 * c
            } else {
                1.055 * c.powf(1.0 / 2.4) - 0.055
            }
        };

        Self {
            r: gamma(r_lin),
            g: gamma(g_lin),
            b: gamma(b_lin),
            a: 1.0,
        }
    }
}

fn float_channel_to_u8(channel: f32) -> u8 {
    let scaled = (channel.clamp(0.0, 1.0) * 255.0).round();
    match scaled.to_u8() {
        Some(value) => value,
        None => {
            if scaled.is_sign_negative() {
                0
            } else {
                u8::MAX
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Color;

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn cmyk_white_black_primaries() {
        let white = Color::from_cmyk(0.0, 0.0, 0.0, 0.0);
        assert!(approx_eq(white.r, 1.0) && approx_eq(white.g, 1.0) && approx_eq(white.b, 1.0));

        let black = Color::from_cmyk(0.0, 0.0, 0.0, 1.0);
        assert!(black.r < 0.25 && black.g < 0.25 && black.b < 0.25);

        let cyan = Color::from_cmyk(1.0, 0.0, 0.0, 0.0);
        assert!(cyan.r < cyan.g && cyan.r < cyan.b);

        let magenta = Color::from_cmyk(0.0, 1.0, 0.0, 0.0);
        assert!(magenta.g < magenta.r && magenta.g < magenta.b);

        let yellow = Color::from_cmyk(0.0, 0.0, 1.0, 0.0);
        assert!(yellow.b < yellow.r && yellow.b < yellow.g);
    }

    #[test]
    fn gray_levels() {
        let black = Color::from_gray(0.0);
        assert!(approx_eq(black.r, 0.0) && approx_eq(black.g, 0.0) && approx_eq(black.b, 0.0));

        let mid = Color::from_gray(0.5);
        assert!(approx_eq(mid.r, 0.5) && approx_eq(mid.g, 0.5) && approx_eq(mid.b, 0.5));

        let white = Color::from_gray(1.0);
        assert!(approx_eq(white.r, 1.0) && approx_eq(white.g, 1.0) && approx_eq(white.b, 1.0));
    }

    #[test]
    fn creates_colors_from_device_components() {
        assert_eq!(
            Color::from_device_components(&[0.5]),
            Some(Color::from_gray(0.5))
        );
        assert_eq!(
            Color::from_device_components(&[0.1, 0.2, 0.3]),
            Some(Color::from_rgb(0.1, 0.2, 0.3))
        );
        assert_eq!(
            Color::from_device_components(&[0.1, 0.2, 0.3, 0.4]),
            Some(Color::from_cmyk(0.1, 0.2, 0.3, 0.4))
        );
    }

    #[test]
    fn rejects_unsupported_device_component_counts() {
        assert_eq!(Color::from_device_components(&[]), None);
        assert_eq!(Color::from_device_components(&[0.1, 0.2]), None);
        assert_eq!(
            Color::from_device_components(&[0.1, 0.2, 0.3, 0.4, 0.5]),
            None
        );
    }

    #[test]
    fn converts_to_rgba8() {
        let color = Color::from_rgba(1.0, 0.5, 0.0, 0.25);

        assert_eq!(color.to_rgba8(), [255, 128, 0, 64]);
    }

    #[test]
    fn rgba8_conversion_clamps_channels() {
        let color = Color::from_rgba(-0.5, 1.5, 0.0, 1.0);

        assert_eq!(color.to_rgba8(), [0, 255, 0, 255]);
    }

    #[test]
    fn lab_white_and_black() {
        // Tolerance for multi-step floating-point conversion.
        let approx = |a: f32, b: f32| (a - b).abs() < 1e-3;

        // D65 white point (CIE standard illuminant).
        let d65 = [0.9505, 1.0, 1.089];

        // L=100, a=0, b=0 is the maximum lightness → pure white.
        let white = Color::from_lab(100.0, 0.0, 0.0, d65);
        assert!(
            approx(white.r, 1.0) && approx(white.g, 1.0) && approx(white.b, 1.0),
            "expected white, got {:?}",
            white
        );

        // L=0, a=0, b=0 is zero lightness → pure black.
        let black = Color::from_lab(0.0, 0.0, 0.0, d65);
        assert!(
            approx(black.r, 0.0) && approx(black.g, 0.0) && approx(black.b, 0.0),
            "expected black, got {:?}",
            black
        );
    }
}
