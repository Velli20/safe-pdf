use crate::{
    compose_op::ComposeOp, error::Jbig2Error, fixed_point::Jbig2Fixed8, image::JBig2Image,
    region_info::RegionInfo,
};
use bitflags::bitflags;
use pdf_utils::BitReader;

const HTEMPLATE_SHIFT: u8 = 1;
const HCOMBOP_SHIFT: u8 = 4;
const HALFTONE_GRID_OVERFLOW: &str = "grid coordinate overflow";

bitflags! {
    /// JBIG2 halftone region flags from T.88 / ISO 14492 section 7.4.5.1.1.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    struct HalftoneRegionFlagBits: u8 {
        const HMMR = 1 << 0;
        const HTEMPLATE_MASK = 0b11 << 1;
        const HENABLESKIP = 1 << 3;
        const HCOMBOP_MASK = 0b111 << 4;
        const HDEFPIXEL = 1 << 7;
    }
}

impl HalftoneRegionFlagBits {
    /// Return the arithmetic template selector from `HTEMPLATE`.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 7.4.5.1.1 stores `HTEMPLATE` in bits
    /// 1 and 2 of the halftone region flags byte.
    fn h_template(self) -> u8 {
        (self.bits() & Self::HTEMPLATE_MASK.bits()) >> HTEMPLATE_SHIFT
    }

    /// Return the halftone composition operator bits from `HCOMBOP`.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 7.4.5.1.1 stores `HCOMBOP` in bits
    /// 4 through 6 of the halftone region flags byte.
    fn h_combop_bits(self) -> u8 {
        (self.bits() & Self::HCOMBOP_MASK.bits()) >> HCOMBOP_SHIFT
    }
}

/// Position of one cell in a JBIG2 halftone grid.
///
/// ITU-T T.88 / ISO/IEC 14492 section 6.6.5 iterates halftone cells by row
/// (`mg`) and column (`ng`) while mapping the decoded gray-scale image to
/// pattern dictionary entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HalftoneGridCell {
    /// Row index in the halftone grid (`mg` in section 6.6.5).
    pub(crate) mg: u16,
    /// Column index in the halftone grid (`ng` in section 6.6.5).
    pub(crate) ng: u16,
}

/// Pixel placement for one halftone pattern cell.
///
/// ITU-T T.88 / ISO/IEC 14492 section 6.6.5 step 5 defines the signed pixel
/// coordinates computed from `HGX`, `HGY`, `HRX`, and `HRY`. Coordinates stay
/// signed because pattern rectangles may be clipped against the region bitmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HalftonePlacement {
    /// Signed X coordinate where the selected pattern is composed.
    pub(crate) x: i32,
    /// Signed Y coordinate where the selected pattern is composed.
    pub(crate) y: i32,
}

/// Row-major iterator over a JBIG2 halftone grid.
///
/// ITU-T T.88 / ISO/IEC 14492 section 6.6.5 treats the halftone gray-scale
/// image as `HGW` by `HGH` grid cells. This iterator yields those cells in the
/// same row-major order used by the placement and composition procedure.
#[derive(Debug, Clone)]
pub(crate) struct HalftoneGridCells {
    width: u16,
    height: u16,
    next: Option<HalftoneGridCell>,
}

impl HalftoneGridCells {
    /// Create a row-major iterator for the `HGW` by `HGH` halftone grid.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 7.4.5.1 stores those dimensions in
    /// the halftone region segment header. Empty grids produce no cells.
    fn new(width: u16, height: u16) -> Self {
        let next = if width == 0 || height == 0 {
            None
        } else {
            Some(HalftoneGridCell { mg: 0, ng: 0 })
        };

        Self {
            width,
            height,
            next,
        }
    }
}

impl Iterator for HalftoneGridCells {
    type Item = HalftoneGridCell;

    /// Yield the next row-major halftone grid cell.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 6.6.5 indexes the gray-scale image by
    /// `(mg, ng)`. This advances `ng` first, then moves to the next `mg` row.
    fn next(&mut self) -> Option<Self::Item> {
        let cell = self.next?;
        let next_ng = cell.ng.checked_add(1)?;
        if next_ng < self.width {
            self.next = Some(HalftoneGridCell {
                mg: cell.mg,
                ng: next_ng,
            });
            return Some(cell);
        }

        let next_mg = cell.mg.checked_add(1)?;
        self.next = if next_mg < self.height {
            Some(HalftoneGridCell { mg: next_mg, ng: 0 })
        } else {
            None
        };
        Some(cell)
    }
}

/// Parsed JBIG2 halftone region header from T.88 / ISO 14492 section 7.4.5.1.
///
/// The header combines the common [`RegionInfo`] fields with halftone-specific
/// flags and fixed-point grid placement parameters used while composing the
/// decoded pattern cells into the region bitmap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HalftoneRegionHeader {
    /// Common region metadata from section 7.4.1.
    pub(crate) region: RegionInfo,
    /// Whether the gray-scale halftone image is MMR encoded.
    pub(crate) mmr: bool,
    /// Arithmetic template selector carried in the halftone flags.
    pub(crate) template: u8,
    /// Whether `HSKIP` generation is enabled for off-page cells.
    pub(crate) enable_skip: bool,
    /// Composition operator used when painting patterns into the region.
    pub(crate) combination_operator: ComposeOp,
    /// Default fill value for the region image before pattern composition.
    pub(crate) default_pixel: bool,
    /// Number of halftone cells along the X axis.
    pub(crate) grid_width: u16,
    /// Number of halftone cells along the Y axis.
    pub(crate) grid_height: u16,
    /// Fixed-point X origin of the halftone grid.
    pub(crate) grid_x: i32,
    /// Fixed-point Y origin of the halftone grid.
    pub(crate) grid_y: i32,
    /// Fixed-point X component of the halftone grid vector.
    pub(crate) grid_vector_x: u16,
    /// Fixed-point Y component of the halftone grid vector.
    pub(crate) grid_vector_y: u16,
}

impl TryFrom<&mut BitReader<'_>> for HalftoneRegionHeader {
    type Error = Jbig2Error;

    /// Parse a halftone region header from the current byte-aligned stream position.
    ///
    /// This reads the header fields defined by T.88 / ISO 14492 section 7.4.5.1,
    /// including the embedded [`RegionInfo`] and the fixed-point grid parameters
    /// used during halftone cell placement.
    fn try_from(stream: &mut BitReader<'_>) -> Result<Self, Self::Error> {
        let region = RegionInfo::parse(stream)?;
        let flags = HalftoneRegionFlagBits::from_bits_retain(stream.try_read_u8::<u8>()?);

        Ok(Self {
            region,
            mmr: flags.contains(HalftoneRegionFlagBits::HMMR),
            template: flags.h_template(),
            enable_skip: flags.contains(HalftoneRegionFlagBits::HENABLESKIP),
            combination_operator: ComposeOp::from(flags.h_combop_bits()),
            default_pixel: flags.contains(HalftoneRegionFlagBits::HDEFPIXEL),
            grid_width: stream.try_read_u32_be::<u16>()?,
            grid_height: stream.try_read_u32_be::<u16>()?,
            grid_x: stream.try_read_i32_be()?,
            grid_y: stream.try_read_i32_be()?,
            grid_vector_x: stream.try_read_u16_be::<u16>()?,
            grid_vector_y: stream.try_read_u16_be::<u16>()?,
        })
    }
}

impl HalftoneRegionHeader {
    /// Iterate over every cell in the halftone grid.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 6.6.5 maps the decoded gray-scale
    /// image to pattern cells over `HGW` columns and `HGH` rows.
    pub(crate) fn cells(&self) -> HalftoneGridCells {
        HalftoneGridCells::new(self.grid_width, self.grid_height)
    }

    /// Compute the halftone pattern X coordinate for grid cell `(mg, ng)`.
    ///
    /// T.88 / ISO 14492 section 6.6.5 step 5 places each pattern at
    /// `x = (HGX + mg x HRY + ng x HRX) >> 8`. The result remains signed
    /// because a valid cell may land partially or fully outside the region
    /// bitmap before clipping or skip processing.
    pub(crate) fn grid_coordinate_signed(&self, mg: u16, ng: u16) -> Result<i32, Jbig2Error> {
        let mg_step = Jbig2Fixed8::from_raw_u16(self.grid_vector_y)
            .checked_mul(mg, HALFTONE_GRID_OVERFLOW)?;
        let ng_step = Jbig2Fixed8::from_raw_u16(self.grid_vector_x)
            .checked_mul(ng, HALFTONE_GRID_OVERFLOW)?;
        Jbig2Fixed8::from_raw_i32(self.grid_x)
            .checked_add(mg_step, HALFTONE_GRID_OVERFLOW)?
            .checked_add(ng_step, HALFTONE_GRID_OVERFLOW)?
            .to_i32_floor(HALFTONE_GRID_OVERFLOW)
    }

    /// Compute the halftone pattern Y coordinate for grid cell `(mg, ng)`.
    ///
    /// T.88 / ISO 14492 section 6.6.5 step 5 places each pattern at
    /// `y = (HGY + mg x HRX - ng x HRY) >> 8`. The result remains signed for
    /// the same reason as [`Self::grid_coordinate_signed`].
    pub(crate) fn grid_coordinate_with_subtract_signed(
        &self,
        mg: u16,
        ng: u16,
    ) -> Result<i32, Jbig2Error> {
        let mg_step = Jbig2Fixed8::from_raw_u16(self.grid_vector_x)
            .checked_mul(mg, HALFTONE_GRID_OVERFLOW)?;
        let ng_step = Jbig2Fixed8::from_raw_u16(self.grid_vector_y)
            .checked_mul(ng, HALFTONE_GRID_OVERFLOW)?;
        Jbig2Fixed8::from_raw_i32(self.grid_y)
            .checked_add(mg_step, HALFTONE_GRID_OVERFLOW)?
            .checked_sub(ng_step, HALFTONE_GRID_OVERFLOW)?
            .to_i32_floor(HALFTONE_GRID_OVERFLOW)
    }

    /// Compute the signed pattern placement for one halftone grid cell.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 6.6.5 step 5 defines both
    /// coordinates from the fixed-point grid origin and vectors before pattern
    /// composition clips the selected pattern into the region bitmap.
    pub(crate) fn placement(
        &self,
        cell: HalftoneGridCell,
    ) -> Result<HalftonePlacement, Jbig2Error> {
        Ok(HalftonePlacement {
            x: self.grid_coordinate_signed(cell.mg, cell.ng)?,
            y: self.grid_coordinate_with_subtract_signed(cell.mg, cell.ng)?,
        })
    }

    /// Build the `HSKIP` bitmap for cells whose pattern rectangle is off-page.
    ///
    /// T.88 / ISO 14492 section 6.6.5 step 2 defines `HSKIP` before gray-image
    /// decoding. A cell is skipped when the placed pattern lies completely
    /// outside the region bitmap bounds.
    pub(crate) fn compute_halftone_skip_map(
        &self,
        pattern_width: u16,
        pattern_height: u16,
    ) -> Result<JBig2Image, Jbig2Error> {
        let mut skip = JBig2Image::try_new(self.grid_width, self.grid_height, None)?;
        let pattern_width = i32::from(pattern_width);
        let pattern_height = i32::from(pattern_height);
        let region_width = i32::from(self.region.width);
        let region_height = i32::from(self.region.height);

        for cell in self.cells() {
            let placement = self.placement(cell)?;
            let off_page = placement.x.saturating_add(pattern_width) <= 0
                || placement.x >= region_width
                || placement.y.saturating_add(pattern_height) <= 0
                || placement.y >= region_height;
            if off_page {
                skip.set_pixel(cell.ng, cell.mg, 1);
            }
        }

        Ok(skip)
    }
}

#[cfg(test)]
mod tests {
    use super::{HCOMBOP_SHIFT, HalftoneGridCell, HalftoneRegionFlagBits, HalftoneRegionHeader};
    use crate::error::Jbig2Error;
    use crate::fixed_point::Jbig2Fixed8;
    use crate::{compose_op::ComposeOp, region_info::RegionInfo};
    use pdf_utils::BitReader;

    const HALFTONE_TEST_HCOMBOP_XOR: u8 = 2 << HCOMBOP_SHIFT;
    const ONE_PIXEL_FIXED8: u16 = 256;
    const TWO_PIXELS_FIXED8: i32 = 512;
    const NEGATIVE_FOUR_PIXELS_FIXED8: i32 = -1024;

    fn test_header() -> HalftoneRegionHeader {
        HalftoneRegionHeader {
            region: RegionInfo {
                width: 2,
                height: 2,
                x: 0,
                y: 0,
                flags: 0,
            },
            mmr: false,
            template: 0,
            enable_skip: false,
            combination_operator: ComposeOp::Or,
            default_pixel: false,
            grid_width: 2,
            grid_height: 2,
            grid_x: 0,
            grid_y: 0,
            grid_vector_x: 0,
            grid_vector_y: 0,
        }
    }

    fn grid_coordinate_unsigned_for_test(
        base: i32,
        mg: u16,
        ng: u16,
        mg_scale: u16,
        ng_scale: u16,
    ) -> Result<u16, Jbig2Error> {
        let overflow_context = "grid coordinate overflow";
        let mg_step = Jbig2Fixed8::from_raw_u16(mg_scale).checked_mul(mg, overflow_context)?;
        let ng_step = Jbig2Fixed8::from_raw_u16(ng_scale).checked_mul(ng, overflow_context)?;
        let coordinate = Jbig2Fixed8::from_raw_i32(base)
            .checked_add(mg_step, overflow_context)?
            .checked_add(ng_step, overflow_context)?
            .to_i32_floor(overflow_context)?;
        u16::try_from(coordinate).map_err(|_| Jbig2Error::Overflow(overflow_context))
    }

    fn grid_coordinate_with_subtract_unsigned_for_test(
        base: i32,
        mg: u16,
        ng: u16,
        mg_scale: u16,
        ng_scale: u16,
    ) -> Result<u16, Jbig2Error> {
        let overflow_context = "grid coordinate overflow";
        let mg_step = Jbig2Fixed8::from_raw_u16(mg_scale).checked_mul(mg, overflow_context)?;
        let ng_step = Jbig2Fixed8::from_raw_u16(ng_scale).checked_mul(ng, overflow_context)?;
        let coordinate = Jbig2Fixed8::from_raw_i32(base)
            .checked_add(mg_step, overflow_context)?
            .checked_sub(ng_step, overflow_context)?
            .to_i32_floor(overflow_context)?;
        u16::try_from(coordinate).map_err(|_| Jbig2Error::Overflow(overflow_context))
    }

    #[test]
    fn parses_halftone_region_header() {
        let mut data = Vec::new();
        data.extend_from_slice(&2u32.to_be_bytes());
        data.extend_from_slice(&3u32.to_be_bytes());
        data.extend_from_slice(&4i32.to_be_bytes());
        data.extend_from_slice(&5i32.to_be_bytes());
        data.push(0x02);
        data.push(HalftoneRegionFlagBits::HENABLESKIP.bits() | HALFTONE_TEST_HCOMBOP_XOR);
        data.extend_from_slice(&6u32.to_be_bytes());
        data.extend_from_slice(&7u32.to_be_bytes());
        data.extend_from_slice(&8i32.to_be_bytes());
        data.extend_from_slice(&9i32.to_be_bytes());
        data.extend_from_slice(&10u16.to_be_bytes());
        data.extend_from_slice(&11u16.to_be_bytes());

        let mut reader = BitReader::new(&data);
        let header = HalftoneRegionHeader::try_from(&mut reader).expect("header");
        assert_eq!(
            header.region,
            RegionInfo {
                width: 2,
                height: 3,
                x: 4,
                y: 5,
                flags: 0x02,
            }
        );
        assert_eq!(header.template, 0);
        assert!(header.enable_skip);
        assert_eq!(header.combination_operator, ComposeOp::Xor);
        assert_eq!(header.grid_width, 6);
        assert_eq!(header.grid_height, 7);
        assert_eq!(header.grid_x, 8);
        assert_eq!(header.grid_y, 9);
        assert_eq!(header.grid_vector_x, 10);
        assert_eq!(header.grid_vector_y, 11);
        assert_eq!(reader.byte_pos(), 38);
    }

    #[test]
    fn parses_halftone_region_header_with_max_grid_dimensions() {
        let mut data = Vec::new();
        data.extend_from_slice(&2u32.to_be_bytes());
        data.extend_from_slice(&3u32.to_be_bytes());
        data.extend_from_slice(&4i32.to_be_bytes());
        data.extend_from_slice(&5i32.to_be_bytes());
        data.push(0x02);
        data.push(0x00);
        data.extend_from_slice(&u32::from(u16::MAX).to_be_bytes());
        data.extend_from_slice(&u32::from(u16::MAX).to_be_bytes());
        data.extend_from_slice(&8i32.to_be_bytes());
        data.extend_from_slice(&9i32.to_be_bytes());
        data.extend_from_slice(&10u16.to_be_bytes());
        data.extend_from_slice(&11u16.to_be_bytes());

        let mut reader = BitReader::new(&data);
        let header = HalftoneRegionHeader::try_from(&mut reader).expect("header");
        assert_eq!(header.grid_width, u16::MAX);
        assert_eq!(header.grid_height, u16::MAX);
        assert_eq!(reader.byte_pos(), 38);
    }

    #[test]
    fn parse_halftone_region_header_truncates_with_byte_aligned_read_error() {
        let mut data = Vec::new();
        data.extend_from_slice(&2u32.to_be_bytes());
        data.extend_from_slice(&3u32.to_be_bytes());
        data.extend_from_slice(&4i32.to_be_bytes());
        data.extend_from_slice(&5i32.to_be_bytes());

        let mut reader = BitReader::new(&data);
        let err = HalftoneRegionHeader::try_from(&mut reader).expect_err("truncated");
        assert_eq!(err, Jbig2Error::Truncated("byte-aligned read"));
    }

    #[test]
    fn computes_skip_cells_for_off_page_patterns() {
        let mut header = test_header();
        header.grid_x = TWO_PIXELS_FIXED8;
        header.grid_y = TWO_PIXELS_FIXED8;
        header.grid_vector_x = ONE_PIXEL_FIXED8;
        header.grid_vector_y = ONE_PIXEL_FIXED8;

        let skip = header.compute_halftone_skip_map(2, 2).expect("skip");
        assert_eq!(skip.get_pixel(0, 0), 1);
        assert_eq!(skip.get_pixel(1, 0), 1);
        assert_eq!(skip.get_pixel(0, 1), 1);
        assert_eq!(skip.get_pixel(1, 1), 1);
    }

    #[test]
    fn skip_map_handles_negative_grid_coordinates() {
        let mut header = test_header();
        header.grid_x = NEGATIVE_FOUR_PIXELS_FIXED8;
        header.grid_vector_x = ONE_PIXEL_FIXED8;

        let skip = header
            .compute_halftone_skip_map(2, 2)
            .expect("negative coordinates");
        assert_eq!(skip.get_pixel(0, 0), 1);
        assert_eq!(skip.get_pixel(1, 0), 1);
        assert_eq!(skip.get_pixel(0, 1), 1);
        assert_eq!(skip.get_pixel(1, 1), 1);
    }

    #[test]
    fn grid_coordinate_returns_u16_for_positive_fixed_point_value() {
        let coordinate =
            grid_coordinate_unsigned_for_test(TWO_PIXELS_FIXED8, 0, 0, 0, 0).expect("coordinate");
        assert_eq!(coordinate, 2);
    }

    #[test]
    fn grid_coordinate_rejects_negative_fixed_point_value() {
        let err = grid_coordinate_with_subtract_unsigned_for_test(0, 0, 1, 0, ONE_PIXEL_FIXED8)
            .expect_err("negative");
        assert_eq!(err, Jbig2Error::Overflow("grid coordinate overflow"));
    }

    #[test]
    fn signed_grid_coordinate_supports_negative_fixed_point_value() {
        let mut header = test_header();
        header.grid_vector_y = ONE_PIXEL_FIXED8;

        let coordinate = header
            .grid_coordinate_with_subtract_signed(0, 1)
            .expect("coordinate");
        assert_eq!(coordinate, -1);
    }

    #[test]
    fn signed_grid_coordinate_supports_negative_grid_origin() {
        let mut header = test_header();
        header.grid_x = -i32::from(ONE_PIXEL_FIXED8);

        let coordinate = header.grid_coordinate_signed(0, 0).expect("coordinate");
        assert_eq!(coordinate, -1);
    }

    #[test]
    fn cells_iterate_halftone_grid_in_row_major_order() {
        let mut header = test_header();
        header.grid_width = 3;
        header.grid_height = 2;

        let cells = header.cells().collect::<Vec<_>>();

        assert_eq!(
            cells,
            vec![
                HalftoneGridCell { mg: 0, ng: 0 },
                HalftoneGridCell { mg: 0, ng: 1 },
                HalftoneGridCell { mg: 0, ng: 2 },
                HalftoneGridCell { mg: 1, ng: 0 },
                HalftoneGridCell { mg: 1, ng: 1 },
                HalftoneGridCell { mg: 1, ng: 2 },
            ]
        );
    }

    #[test]
    fn placement_combines_grid_coordinate_components() {
        let mut header = test_header();
        header.grid_x = ONE_PIXEL_FIXED8.into();
        header.grid_y = TWO_PIXELS_FIXED8;
        header.grid_vector_x = ONE_PIXEL_FIXED8;
        header.grid_vector_y = ONE_PIXEL_FIXED8;

        let placement = header
            .placement(HalftoneGridCell { mg: 1, ng: 2 })
            .expect("placement");

        assert_eq!(placement.x, 4);
        assert_eq!(placement.y, 1);
    }

    #[test]
    fn grid_coordinate_rejects_values_above_u16_max() {
        let err = grid_coordinate_unsigned_for_test(
            i32::from(u16::MAX)
                .checked_mul(i32::from(ONE_PIXEL_FIXED8))
                .and_then(|value| value.checked_add(i32::from(ONE_PIXEL_FIXED8)))
                .expect("test fixed-point value"),
            0,
            0,
            0,
            0,
        )
        .expect_err("coordinate overflow");
        assert_eq!(err, Jbig2Error::Overflow("grid coordinate overflow"));
    }
}
