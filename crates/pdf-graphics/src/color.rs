/// Unpremultiplied color with RGBA channel
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    /// Returs color value from rgba component values.
    ///
    /// # Arguments
    ///
    /// - 'r': Value of red channel. the value needs between [0.0, 1.0]
    /// - 'g': Value of green channel. the value needs between [0.0, 1.0]
    /// - 'b': Value of blue channel. the value needs between [0.0, 1.0]
    /// - 'a': Value of alpha channel. the value needs between [0.0, 1.0]
    pub const fn from_rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn from_rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
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
    /// All component values should be in the range [0.0, 1.0]. Conversion uses
    /// the standard formula: r = (1 - c) * (1 - k), g = (1 - m) * (1 - k),
    /// b = (1 - y) * (1 - k).
    /// Alpha defaults to 1.0 (opaque).
    pub const fn from_cmyk(c: f32, m: f32, y: f32, k: f32) -> Self {
        let r = (1.0 - c) * (1.0 - k);
        let g = (1.0 - m) * (1.0 - k);
        let b = (1.0 - y) * (1.0 - k);
        Self { r, g, b, a: 1.0 }
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
        assert!(approx_eq(black.r, 0.0) && approx_eq(black.g, 0.0) && approx_eq(black.b, 0.0));

        let cyan = Color::from_cmyk(1.0, 0.0, 0.0, 0.0);
        assert!(approx_eq(cyan.r, 0.0) && approx_eq(cyan.g, 1.0) && approx_eq(cyan.b, 1.0));

        let magenta = Color::from_cmyk(0.0, 1.0, 0.0, 0.0);
        assert!(
            approx_eq(magenta.r, 1.0) && approx_eq(magenta.g, 0.0) && approx_eq(magenta.b, 1.0)
        );

        let yellow = Color::from_cmyk(0.0, 0.0, 1.0, 0.0);
        assert!(approx_eq(yellow.r, 1.0) && approx_eq(yellow.g, 1.0) && approx_eq(yellow.b, 0.0));
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
}
