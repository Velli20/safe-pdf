use crate::error::FilterError;
use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

/// Parameters for the Predictor post-processing step shared by
/// LZWDecode and FlateDecode (PDF spec §7.4.4.4, Table 10).
#[derive(Debug, Clone)]
pub(crate) struct PredictorParams {
    /// Predictor algorithm (1 = none, 2 = TIFF, 10–15 = PNG).
    pub predictor: u8,
    /// Number of interleaved colour components per sample.
    pub colors: usize,
    /// Bits per colour component.
    pub bits_per_component: usize,
    /// Number of samples per row.
    pub columns: usize,
}

impl Default for PredictorParams {
    fn default() -> Self {
        Self {
            predictor: 1,
            colors: 1,
            bits_per_component: 8,
            columns: 1,
        }
    }
}

impl PredictorParams {
    /// Parse predictor parameters from a `/DecodeParms` dictionary.
    ///
    /// Missing keys fall back to the PDF-specified defaults.
    pub fn from_dictionary(
        dict: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, FilterError> {
        let mut p = Self::default();

        if let Some(value) = dict.optional_number::<u8>(b"Predictor", objects)? {
            p.predictor = value;
        }
        if let Some(value) = dict.optional_number::<usize>(b"Colors", objects)? {
            p.colors = value;
        }
        if let Some(value) = dict.optional_number::<usize>(b"BitsPerComponent", objects)? {
            p.bits_per_component = value;
        }
        if let Some(value) = dict.optional_number::<usize>(b"Columns", objects)? {
            p.columns = value;
        }

        Ok(p)
    }

    /// Returns `true` if no prediction is needed.
    pub fn is_none(&self) -> bool {
        self.predictor == 1
    }

    /// Number of bytes per complete row of sample data (before any filter byte).
    fn row_bytes(&self) -> usize {
        let bits_per_pixel = self.colors.saturating_mul(self.bits_per_component);
        let total_bits = self.columns.saturating_mul(bits_per_pixel);
        total_bits.saturating_add(7) / 8
    }

    /// Bytes per pixel, rounded up (used by PNG sub/average/paeth filters).
    fn bpp(&self) -> usize {
        let bits = self.colors.saturating_mul(self.bits_per_component);
        bits.saturating_add(7) / 8
    }
}

/// Applies the predictor algorithm to `data`, returning the reconstructed bytes.
///
/// If `params.predictor` is 1 (no prediction), the data is returned unchanged.
///
/// # Errors
///
/// Returns [`FilterError::Decompression`] if the data length is inconsistent
/// with the declared row width.
pub(crate) fn apply_predictor(
    data: &[u8],
    params: &PredictorParams,
) -> Result<Vec<u8>, FilterError> {
    match params.predictor {
        1 => Ok(data.to_vec()),
        2 => apply_tiff_predictor(data, params),
        10..=15 => apply_png_predictor(data, params),
        other => Err(FilterError::Decompression(format!(
            "unsupported Predictor value {other}"
        ))),
    }
}

/// TIFF Predictor 2: horizontal differencing (reverse).
///
/// Each sample is stored as the difference from the previous sample in the
/// row. We reconstruct by accumulating across each row.
fn apply_tiff_predictor(data: &[u8], params: &PredictorParams) -> Result<Vec<u8>, FilterError> {
    if params.bits_per_component != 8 {
        return Err(FilterError::Decompression(
            "TIFF Predictor 2 only supports 8 bits per component".into(),
        ));
    }

    let row_bytes = params.row_bytes();
    if row_bytes == 0 {
        return Ok(Vec::new());
    }

    let colors = params.colors;
    let mut output = data.to_vec();

    for row in output.chunks_exact_mut(row_bytes) {
        // The first `colors` bytes in each row are absolute; subsequent bytes
        // are differences that we accumulate.
        for i in colors..row.len() {
            let Some(&prev) = row.get(i.wrapping_sub(colors)) else {
                continue;
            };
            if let Some(cur) = row.get_mut(i) {
                *cur = cur.wrapping_add(prev);
            }
        }
    }

    Ok(output)
}

/// PNG predictor: each row is prefixed by a filter-type byte.
///
/// Filter types (per PNG spec):
/// - 0: None
/// - 1: Sub (difference from left pixel)
/// - 2: Up (difference from pixel above)
/// - 3: Average of left and above
/// - 4: Paeth predictor
fn apply_png_predictor(data: &[u8], params: &PredictorParams) -> Result<Vec<u8>, FilterError> {
    let row_bytes = params.row_bytes();
    // Each PNG-predicted row has a 1-byte filter prefix.
    let stride = row_bytes.saturating_add(1);
    if stride == 0 {
        return Ok(Vec::new());
    }

    let bpp = params.bpp().max(1);
    let num_rows = data.len().checked_div(stride).unwrap_or(0);
    let mut output = Vec::with_capacity(num_rows.saturating_mul(row_bytes));
    let mut prev_row: Vec<u8> = vec![0u8; row_bytes];

    let mut offset = 0usize;
    for _ in 0..num_rows {
        let Some(&filter_type) = data.get(offset) else {
            break;
        };
        offset = offset.saturating_add(1);

        let row_end = offset.saturating_add(row_bytes);
        let row_data = data.get(offset..row_end).unwrap_or_default();

        let mut current_row = vec![0u8; row_bytes];

        for i in 0..row_bytes {
            let raw = row_data.get(i).copied().unwrap_or(0);
            let left = if i >= bpp {
                current_row.get(i.wrapping_sub(bpp)).copied().unwrap_or(0)
            } else {
                0
            };
            let above = prev_row.get(i).copied().unwrap_or(0);
            let upper_left = if i >= bpp {
                prev_row.get(i.wrapping_sub(bpp)).copied().unwrap_or(0)
            } else {
                0
            };

            let reconstructed = match filter_type {
                0 => raw,
                1 => raw.wrapping_add(left),
                2 => raw.wrapping_add(above),
                3 => {
                    let avg = ((u16::from(left)).saturating_add(u16::from(above))) / 2;
                    #[allow(clippy::as_conversions)]
                    let avg_byte = avg as u8; // safe: avg of two u8s / 2 fits in u8
                    raw.wrapping_add(avg_byte)
                }
                4 => raw.wrapping_add(paeth_predictor(left, above, upper_left)),
                _ => raw, // Unknown filter type — treat as None.
            };

            if let Some(slot) = current_row.get_mut(i) {
                *slot = reconstructed;
            }
        }

        output.extend_from_slice(&current_row);
        prev_row = current_row;
        offset = row_end;
    }

    Ok(output)
}

/// Paeth predictor function (PNG spec §9.4).
fn paeth_predictor(a: u8, b: u8, c: u8) -> u8 {
    let pa = (i16::from(b)).wrapping_sub(i16::from(c)).abs();
    let pb = (i16::from(a)).wrapping_sub(i16::from(c)).abs();
    let pc = (i16::from(a))
        .wrapping_add(i16::from(b))
        .wrapping_sub(i16::from(c))
        .wrapping_sub(i16::from(c))
        .abs();
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_params_with(
        predictor: u8,
        colors: usize,
        bpc: usize,
        columns: usize,
    ) -> PredictorParams {
        PredictorParams {
            predictor,
            colors,
            bits_per_component: bpc,
            columns,
        }
    }

    // ---- No prediction ----

    #[test]
    fn predictor_none_returns_unchanged() {
        let data = vec![1, 2, 3, 4, 5];
        let params = default_params_with(1, 1, 8, 5);
        let result = apply_predictor(&data, &params).unwrap();
        assert_eq!(result, data);
    }

    // ---- TIFF Predictor 2 ----

    #[test]
    fn tiff_predictor_single_color() {
        // 1 color, 8 bpc, 4 columns → row_bytes = 4
        // Input differences: [10, 5, 3, 2]
        // Reconstructed:     [10, 15, 18, 20]
        let params = default_params_with(2, 1, 8, 4);
        let data = vec![10, 5, 3, 2];
        let result = apply_predictor(&data, &params).unwrap();
        assert_eq!(result, vec![10, 15, 18, 20]);
    }

    #[test]
    fn tiff_predictor_three_colors() {
        // 3 colors, 8 bpc, 2 columns → row_bytes = 6
        // Row: [R0, G0, B0, dR1, dG1, dB1]
        // R0=10, G0=20, B0=30, dR1=1, dG1=2, dB1=3
        // Reconstructed: [10, 20, 30, 11, 22, 33]
        let params = default_params_with(2, 3, 8, 2);
        let data = vec![10, 20, 30, 1, 2, 3];
        let result = apply_predictor(&data, &params).unwrap();
        assert_eq!(result, vec![10, 20, 30, 11, 22, 33]);
    }

    #[test]
    fn tiff_predictor_multiple_rows() {
        // 1 color, 8 bpc, 3 columns → row_bytes = 3
        // Two rows: [10, 5, 3] [20, 1, 1]
        // Reconstructed: [10, 15, 18] [20, 21, 22]
        let params = default_params_with(2, 1, 8, 3);
        let data = vec![10, 5, 3, 20, 1, 1];
        let result = apply_predictor(&data, &params).unwrap();
        assert_eq!(result, vec![10, 15, 18, 20, 21, 22]);
    }

    // ---- PNG predictors ----

    #[test]
    fn png_none_filter() {
        // Filter 0 (None): raw bytes pass through.
        // 1 color, 8 bpc, 3 columns → row_bytes = 3, stride = 4
        let params = default_params_with(10, 1, 8, 3);
        let data = vec![0, 10, 20, 30]; // filter_byte=0, data=[10,20,30]
        let result = apply_predictor(&data, &params).unwrap();
        assert_eq!(result, vec![10, 20, 30]);
    }

    #[test]
    fn png_sub_filter() {
        // Filter 1 (Sub): each byte is difference from left (bpp=1 for 1 color, 8 bpc).
        // Input: [10, 5, 3]
        // Reconstructed: [10, 15, 18]
        let params = default_params_with(11, 1, 8, 3);
        let data = vec![1, 10, 5, 3];
        let result = apply_predictor(&data, &params).unwrap();
        assert_eq!(result, vec![10, 15, 18]);
    }

    #[test]
    fn png_up_filter() {
        // Filter 2 (Up): each byte is difference from the byte above.
        // Two rows, 3 columns.
        // Row 0 (filter=0 None): [10, 20, 30]
        // Row 1 (filter=2 Up):   [1,  2,  3] → [11, 22, 33]
        let params = default_params_with(12, 1, 8, 3);
        let data = vec![
            0, 10, 20, 30, // row 0: None
            2, 1, 2, 3, // row 1: Up
        ];
        let result = apply_predictor(&data, &params).unwrap();
        assert_eq!(result, vec![10, 20, 30, 11, 22, 33]);
    }

    #[test]
    fn png_average_filter() {
        // Filter 3 (Average): avg(left, above)
        // Row 0 (None):    [10, 20, 30]
        // Row 1 (Average): raw = [5, 5, 5]
        //   i=0: left=0, above=10 → avg=5, 5+5=10
        //   i=1: left=10, above=20 → avg=15, 5+15=20
        //   i=2: left=20, above=30 → avg=25, 5+25=30
        let params = default_params_with(13, 1, 8, 3);
        let data = vec![
            0, 10, 20, 30, // row 0: None
            3, 5, 5, 5, // row 1: Average
        ];
        let result = apply_predictor(&data, &params).unwrap();
        assert_eq!(result, vec![10, 20, 30, 10, 20, 30]);
    }

    #[test]
    fn png_paeth_filter() {
        // Filter 4 (Paeth): uses Paeth predictor function.
        // Row 0 (None): [10, 20, 30]
        // Row 1 (Paeth): raw = [0, 0, 0]
        //   i=0: a=0, b=10, c=0 → paeth=10, 0+10=10
        //   i=1: a=10, b=20, c=10 → paeth=20, 0+20=20
        //   i=2: a=20, b=30, c=20 → paeth=30, 0+30=30
        let params = default_params_with(14, 1, 8, 3);
        let data = vec![
            0, 10, 20, 30, // row 0: None
            4, 0, 0, 0, // row 1: Paeth
        ];
        let result = apply_predictor(&data, &params).unwrap();
        assert_eq!(result, vec![10, 20, 30, 10, 20, 30]);
    }

    #[test]
    fn paeth_predictor_fn_basic() {
        // When a=0, b=0, c=0: pa=0, pb=0, pc=0 → returns a=0
        assert_eq!(paeth_predictor(0, 0, 0), 0);
        // When a=10, b=20, c=10: pa=|20-10|=10, pb=|10-10|=0, pc=|10+20-20|=10
        // pb is smallest → returns b=20
        assert_eq!(paeth_predictor(10, 20, 10), 20);
    }

    #[test]
    fn png_sub_filter_rgb() {
        // 3 colors, 8 bpc, 2 columns → row_bytes = 6, bpp = 3
        // Filter 1 (Sub): each byte is difference from byte 3 positions left.
        // Input: [10, 20, 30, 1, 2, 3]
        // Reconstructed: [10, 20, 30, 11, 22, 33]
        let params = default_params_with(11, 3, 8, 2);
        let data = vec![1, 10, 20, 30, 1, 2, 3];
        let result = apply_predictor(&data, &params).unwrap();
        assert_eq!(result, vec![10, 20, 30, 11, 22, 33]);
    }
}
