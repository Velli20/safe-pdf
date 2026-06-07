use crate::{
    arith_decoder::{JBig2ArithDecoder, template_refs::Opt3Refs},
    error::Jbig2Error,
    image::JBig2Image,
};

use super::tables::{OPT3_BITS_DESCENDING, Opt3TemplateConfig};

const BITS_PER_PACKED_BYTE: usize = 8;
const PACKED_BYTE_ROUNDING_BIAS: usize = BITS_PER_PACKED_BYTE - 1;
const PACKED_BYTE_SHIFT: u32 = 3;

impl JBig2ArithDecoder<'_, '_> {
    /// Decode an optimized arithmetic generic region for the default template layouts.
    ///
    /// This is the byte-oriented fast path for the T.88 / ISO/IEC 14492
    /// section 6.2.5.7 arithmetic generic-region procedure when `GBTEMPLATE`
    /// uses the default adaptive pixels covered by `Opt3TemplateConfig`.
    /// The row loop implements section 6.2.5.5 line typical prediction
    /// (`TPGDON`/`LTP`) and the template context geometry from section
    /// 6.2.5.3. This path is only used when there is no `SKIP` bitmap.
    pub(super) fn decode_arith_opt3(
        &mut self,
        width: u16,
        height: u16,
        tpgdon: bool,
        config: Opt3TemplateConfig,
    ) -> Result<JBig2Image, Jbig2Error> {
        let mut image = JBig2Image::try_new(width, height, None)?;
        let row = Opt3ImageRows::new(width, height)?;
        let mut row_buffer = Vec::new();
        row_buffer
            .try_reserve_exact(row.byte_len())
            .map_err(|_| Jbig2Error::Allocation("generic region row"))?;
        let mut ltp = 0u8;
        self.ensure_generic_region_contexts()?;

        for row_index in 0..row.height {
            if tpgdon {
                ltp ^= self.decode_prepared_generic_context(config.tpgd_context)?;
            }
            if ltp != 0 {
                image.copy_line(row_index, row_index.saturating_sub(1));
                continue;
            }

            let row_info = row.info(row_index);
            if row_index > 1 {
                self.decode_row_with_two_refs(&mut image, row_info, &mut row_buffer, config)?;
            } else {
                self.decode_row_with_one_ref(&mut image, row_info, &mut row_buffer, config)?;
            }
        }

        Ok(image)
    }

    /// Decode one optimized row once both previous reference rows are available.
    ///
    /// For rows after the first two, T.88 / ISO/IEC 14492 section 6.2.5.3
    /// template contexts can be updated from both preceding decoded rows. This
    /// method seeds those row references, decodes section 6.2.5.7 pixels into a
    /// temporary byte row, and writes the completed row into the image.
    fn decode_row_with_two_refs(
        &mut self,
        image: &mut JBig2Image,
        row: Opt3RowInfo,
        row_buffer: &mut Vec<u8>,
        config: Opt3TemplateConfig,
    ) -> Result<(), Jbig2Error> {
        let mut refs = Opt3Refs::with_two_refs(image, row.row, config)?;
        let mut context = refs.context_with_two_refs(config);

        self.decode_row_bytes(row, row_buffer, config, &mut context, |bit| {
            refs.advance_with_two_refs(image, bit, config)
        })?;
        image.copy_row_prefix_from_slice(row.row, row_buffer)
    }

    /// Decode one optimized row while only the immediately preceding row applies.
    ///
    /// The first two rows do not have the two-row history used by the steady
    /// state template update. This method preserves the section 6.2.5.3
    /// context layout by seeding the available reference pixels only, then
    /// running the section 6.2.5.7 arithmetic pixel loop.
    fn decode_row_with_one_ref(
        &mut self,
        image: &mut JBig2Image,
        row: Opt3RowInfo,
        row_buffer: &mut Vec<u8>,
        config: Opt3TemplateConfig,
    ) -> Result<(), Jbig2Error> {
        let mut refs = Opt3Refs::with_one_ref(image, row.row)?;
        let mut context = refs.context_with_one_ref(config);

        self.decode_row_bytes(row, row_buffer, config, &mut context, |bit| {
            refs.advance_with_one_ref(image, bit, config)
        })?;
        image.copy_row_prefix_from_slice(row.row, row_buffer)
    }

    /// Decode the bytes that make up a single optimized generic-region row.
    ///
    /// T.88 / ISO/IEC 14492 section 6.2.5.7 defines pixels in raster order;
    /// this helper keeps the same left-to-right decode order while packing the
    /// output bits into full bytes plus the final partial byte.
    fn decode_row_bytes<F>(
        &mut self,
        row: Opt3RowInfo,
        bytes: &mut Vec<u8>,
        config: Opt3TemplateConfig,
        context: &mut u32,
        mut update_refs: F,
    ) -> Result<(), Jbig2Error>
    where
        F: FnMut(Option<u32>) -> Result<u32, Jbig2Error>,
    {
        bytes.clear();
        for _ in 0..row.n_line_bytes {
            let byte = self.decode_full_byte(config, context, &mut update_refs)?;
            bytes.push(byte);
        }

        let _ = update_refs(None)?;
        let byte = self.decode_tail_byte(row.n_bits_left, config, context, &mut update_refs)?;
        bytes.push(byte);
        Ok(())
    }

    /// Decode one full output byte from eight arithmetic-coded generic pixels.
    ///
    /// The bits are decoded in the same raster order as T.88 / ISO/IEC 14492
    /// section 6.2.5.7, with `OPT3_BITS_DESCENDING` mapping each decoded pixel
    /// into its packed output-byte position.
    fn decode_full_byte<F>(
        &mut self,
        config: Opt3TemplateConfig,
        context: &mut u32,
        update_refs: &mut F,
    ) -> Result<u8, Jbig2Error>
    where
        F: FnMut(Option<u32>) -> Result<u32, Jbig2Error>,
    {
        let mut byte = 0u8;
        for bit in OPT3_BITS_DESCENDING {
            byte |= self.decode_pixel_bit(bit, config, context, update_refs)?;
        }
        Ok(byte)
    }

    /// Decode the final partial output byte of an optimized generic-region row.
    ///
    /// Section 6.2.5.7 decodes exactly `GBW` pixels per row. This helper emits
    /// only the remaining row pixels and leaves the unused low-order byte bits
    /// clear.
    fn decode_tail_byte<F>(
        &mut self,
        n_bits_left: usize,
        config: Opt3TemplateConfig,
        context: &mut u32,
        update_refs: &mut F,
    ) -> Result<u8, Jbig2Error>
    where
        F: FnMut(Option<u32>) -> Result<u32, Jbig2Error>,
    {
        let mut byte = 0u8;
        for out_bit in OPT3_BITS_DESCENDING.into_iter().take(n_bits_left) {
            byte |= self.decode_pixel_bit(out_bit, config, context, update_refs)?;
        }
        Ok(byte)
    }

    /// Decode one optimized generic-region pixel and advance its context.
    ///
    /// This is the per-pixel arithmetic decode step from T.88 / ISO/IEC 14492
    /// section 6.2.5.7. The decoded pixel is fed back into the template context
    /// defined by section 6.2.5.3 along with the row-reference contribution
    /// supplied by `update_refs`.
    fn decode_pixel_bit<F>(
        &mut self,
        bit: u32,
        config: Opt3TemplateConfig,
        context: &mut u32,
        update_refs: &mut F,
    ) -> Result<u8, Jbig2Error>
    where
        F: FnMut(Option<u32>) -> Result<u32, Jbig2Error>,
    {
        let context_index = usize::try_from(*context)
            .map_err(|_| Jbig2Error::Overflow("generic region context index"))?;
        let pixel = self.decode_prepared_generic_context(context_index)?;
        let shifted_pixel = pixel
            .checked_shl(bit)
            .ok_or(Jbig2Error::InvalidState("bit shift"))?;
        *context = ((*context & config.update_context_mask) << 1)
            | u32::from(pixel)
            | update_refs(Some(bit))?;
        Ok(shifted_pixel)
    }
}

#[derive(Debug, Clone, Copy)]
struct Opt3ImageRows {
    /// Number of rows in the optimized generic-region image.
    height: u16,
    /// Number of complete output bytes before the final partial byte.
    n_line_bytes: usize,
    /// Number of significant bits in the final output byte.
    n_bits_left: usize,
}

impl Opt3ImageRows {
    /// Compute byte packing metadata for optimized generic-region rows.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 6.2.5.7 decodes exactly `GBW`
    /// pixels per row. The optimized path packs those pixels into full bytes
    /// plus one final byte that may contain fewer than eight significant bits.
    fn new(width: u16, height: u16) -> Result<Self, Jbig2Error> {
        let width = usize::from(width);
        let line_bytes = width
            .checked_add(PACKED_BYTE_ROUNDING_BIAS)
            .map(|value| value >> PACKED_BYTE_SHIFT)
            .ok_or(Jbig2Error::Overflow("image dimensions overflow"))?;
        let Some(n_line_bytes) = line_bytes.checked_sub(1) else {
            return Ok(Self {
                height,
                n_line_bytes: 0,
                n_bits_left: 0,
            });
        };
        let n_bits_left = width
            .checked_sub(
                n_line_bytes
                    .checked_mul(BITS_PER_PACKED_BYTE)
                    .ok_or(Jbig2Error::Overflow("image dimensions overflow"))?,
            )
            .ok_or(Jbig2Error::Overflow("image dimensions overflow"))?;

        Ok(Self {
            height,
            n_line_bytes,
            n_bits_left,
        })
    }

    /// Return row-specific packing metadata for `row`.
    fn info(self, row: u16) -> Opt3RowInfo {
        Opt3RowInfo {
            row,
            n_line_bytes: self.n_line_bytes,
            n_bits_left: self.n_bits_left,
        }
    }

    /// Return the temporary row buffer length in packed bytes.
    fn byte_len(self) -> usize {
        self.n_line_bytes.saturating_add(1)
    }
}

#[derive(Debug, Clone, Copy)]
struct Opt3RowInfo {
    /// Row index being decoded.
    row: u16,
    /// Number of complete output bytes before the final partial byte.
    n_line_bytes: usize,
    /// Number of significant bits in the final output byte.
    n_bits_left: usize,
}

#[cfg(test)]
mod tests {
    use pdf_utils::BitReader;

    use crate::{
        arith_decoder::JBig2ArithDecoder,
        generic_region::{
            GenericRegionAdaptiveTemplate, GenericRegionTemplate,
            tables::{Opt3TemplateConfig, Template12Config},
        },
    };

    #[test]
    fn template0_unoptimized_matches_opt3_default_template() {
        let data = [0x84, 0xc7, 0x73, 0xbf, 0xff, 0xac];
        let template =
            GenericRegionAdaptiveTemplate::from(&[], 0, true, GenericRegionTemplate::Template0)
                .expect("template");

        let mut opt3_stream = BitReader::new(&data);
        let mut opt3_decoder = JBig2ArithDecoder::new(&mut opt3_stream);
        let opt3 = opt3_decoder
            .decode_arith_opt3(8, 4, false, Opt3TemplateConfig::TEMPLATE0)
            .expect("opt3 decode");

        let mut unopt_stream = BitReader::new(&data);
        let mut unopt_decoder = JBig2ArithDecoder::new(&mut unopt_stream);
        let unopt = unopt_decoder
            .decode_arith_template0_unopt_skip(8, 4, false, &template, None)
            .expect("unoptimized decode");

        assert_eq!(unopt, opt3);
    }

    #[test]
    fn template2_unoptimized_matches_opt3_default_template() {
        let data = [0x9a, 0x33, 0x55, 0xe1, 0x0f, 0xff, 0xac];
        let template =
            GenericRegionAdaptiveTemplate::from(&[], 0, true, GenericRegionTemplate::Template2)
                .expect("template");

        let mut opt3_stream = BitReader::new(&data);
        let mut opt3_decoder = JBig2ArithDecoder::new(&mut opt3_stream);
        let opt3 = opt3_decoder
            .decode_arith_opt3(8, 4, false, Opt3TemplateConfig::TEMPLATE2)
            .expect("opt3 decode");

        let mut unopt_stream = BitReader::new(&data);
        let mut unopt_decoder = JBig2ArithDecoder::new(&mut unopt_stream);
        let unopt = unopt_decoder
            .decode_arith_template12_unopt_skip(
                8,
                4,
                Template12Config::TEMPLATE2,
                false,
                &template,
                None,
            )
            .expect("unoptimized decode");

        assert_eq!(unopt, opt3);
    }
}
