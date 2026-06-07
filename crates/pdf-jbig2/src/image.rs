//! 1-bit bitmap utilities for JBIG2 decoding.
//!
//! The bitmap storage keeps each row 4-byte aligned, matching the PDFium
//! internal representation. The decoder later converts it to tightly packed
//! row bytes for the PDF filter output.

use core::cmp::min;
use core::ops::Range;

use crate::{
    compose_op::ComposeOp,
    error::Jbig2Error,
    generic_region::GenericRegionAdaptiveTemplate,
    util::{IMAGE_DIMENSIONS_OVERFLOW, packed_row_len},
};
use pdf_utils::BitReader;

const BITS_PER_BYTE: u16 = 8;
const ROW_ALIGNMENT_BITS: u32 = 32;
const ROW_ALIGNMENT_BYTES: u32 = 4;
const ROW_ALIGNMENT_ROUNDING_BITS: u32 = ROW_ALIGNMENT_BITS - 1;
const MSB_FIRST_BIT_INDEX: u16 = BITS_PER_BYTE - 1;
const WHITE_PIXEL: u8 = 0;
const BLACK_PIXEL: u8 = 1;
const EMPTY_BYTE: u8 = 0x00;
const FULL_BYTE: u8 = 0xff;
const IMAGE_BITMAP_ALLOCATION: &str = "image bitmap";
const ROW_REFERENCE: &str = "row reference";
const ROW_WRITE: &str = "row write";
const COLLECTIVE_BITMAP: &str = "collective bitmap";
const BYTE_ALIGNMENT_MASK: u16 = 7;

/// A decoded JBIG2 1-bit bitmap.
///
/// JBIG2 region procedures such as T.88 / ISO/IEC 14492 section 7.4.6 decode
/// bi-level pixels with `0` for white and `1` for black. This implementation
/// stores those pixels MSB-first in each byte and keeps each internal row
/// 4-byte aligned; callers use [`Self::to_tight_bytes`] when the PDF filter
/// pipeline needs tightly packed section 7 bitmap rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JBig2Image {
    width: u16,
    height: u16,
    stride: u16,
    data: Vec<u8>,
}

impl JBig2Image {
    /// Create an empty bitmap with no storage.
    ///
    /// This represents absent or zero-sized JBIG2 image data; no JBIG2 section
    /// allocates pixels for a zero width or height region.
    pub(crate) fn empty() -> Self {
        Self {
            width: 0,
            height: 0,
            stride: 0,
            data: Vec::new(),
        }
    }

    /// Create a test bitmap, returning an empty bitmap if allocation fails.
    ///
    /// Production code uses [`Self::try_new`] so JBIG2 section 7 region
    /// allocation errors are propagated instead of hidden.
    #[cfg(test)]
    pub(crate) fn new(width: u16, height: u16) -> Self {
        Self::try_new(width, height, None).unwrap_or_else(|_| Self::empty())
    }

    /// Allocate a bitmap for decoded JBIG2 region pixels.
    ///
    /// T.88 / ISO/IEC 14492 section 7 region segment information supplies
    /// width and height. The decoder stores rows 4-byte aligned internally and
    /// rejects overflow before reserving the backing bytes. `default_pixel`
    /// seeds the backing storage when present; `None` preserves the existing
    /// zero-filled allocation behavior.
    pub(crate) fn try_new(
        width: u16,
        height: u16,
        default_pixel: Option<bool>,
    ) -> Result<Self, Jbig2Error> {
        let geometry = BitmapGeometry::new(width, height)?;
        if geometry.is_empty() {
            return Ok(Self::empty());
        }

        let mut data = Vec::new();
        data.try_reserve_exact(geometry.byte_len)
            .map_err(|_| Jbig2Error::Allocation(IMAGE_BITMAP_ALLOCATION))?;
        let fill_byte = if default_pixel.unwrap_or(false) {
            FULL_BYTE
        } else {
            EMPTY_BYTE
        };
        data.resize(geometry.byte_len, fill_byte);

        Ok(Self {
            width,
            height,
            stride: geometry.stride,
            data,
        })
    }

    /// Return whether JBIG2 section 7 region dimensions describe a non-empty bitmap.
    pub(crate) fn is_valid_image_size(width: u16, height: u16) -> bool {
        width > 0 && height > 0
    }

    /// Return the bitmap width from the JBIG2 region or page image.
    pub(crate) fn width(&self) -> u16 {
        self.width
    }

    /// Return the bitmap height from the JBIG2 region or page image.
    pub(crate) fn height(&self) -> u16 {
        self.height
    }

    /// Return the 4-byte-aligned internal row length in bytes.
    ///
    /// JBIG2 row data is conceptually tightly packed, but this decoder keeps
    /// aligned internal rows and strips padding in [`Self::to_tight_bytes`].
    pub(crate) fn stride(&self) -> u16 {
        self.stride
    }

    /// Return the raw aligned bitmap storage.
    ///
    /// Bytes are MSB-first 1-bit pixels. Each row contains [`Self::stride`]
    /// bytes, including implementation padding beyond the JBIG2 image width.
    pub(crate) fn data(&self) -> &[u8] {
        &self.data
    }

    /// Return mutable raw aligned bitmap storage.
    ///
    /// This is used by decoders that already operate on complete JBIG2 row
    /// bytes, such as MMR and optimized generic-region paths.
    pub(crate) fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }

    /// Return the tightly packed row length in bytes.
    ///
    /// T.88 / ISO/IEC 14492 section 7 bitmap rows are MSB-first and rounded to
    /// whole bytes. This excludes the decoder's 4-byte alignment padding.
    pub(crate) fn row_bytes(&self) -> u16 {
        BitmapGeometry::tight_row_bytes(self.width).unwrap_or(0)
    }

    /// Borrow one aligned internal row.
    ///
    /// Returns `None` when `y` is outside the JBIG2 bitmap height or the
    /// internal storage invariant has already been violated.
    pub(crate) fn get_line(&self, y: u16) -> Option<&[u8]> {
        self.row_byte_range(y)
            .and_then(|range| self.data.get(range))
    }

    /// Mutably borrow one aligned internal row.
    ///
    /// Returns `None` when `y` is outside the JBIG2 bitmap height or the
    /// internal storage invariant has already been violated.
    pub(crate) fn get_line_mut(&mut self, y: u16) -> Option<&mut [u8]> {
        self.row_byte_range(y)
            .and_then(|range| self.data.get_mut(range))
    }

    /// Read one byte from an aligned row for JBIG2 arithmetic context templates.
    ///
    /// T.88 Annex A generic-region templates reference prior decoded pixels;
    /// optimized context code reads the already-decoded aligned row bytes
    /// through this checked helper.
    pub(crate) fn read_row_byte(&self, row: u16, byte_offset: usize) -> Result<u8, Jbig2Error> {
        self.get_line(row)
            .and_then(|line| line.get(byte_offset).copied())
            .ok_or(Jbig2Error::InvalidState(ROW_REFERENCE))
    }

    /// Copy decoded JBIG2 row bytes into the start of an aligned image row.
    ///
    /// Optimized generic-region decoding from T.88 section 7.4.6 produces
    /// tightly packed row bytes and leaves the decoder-owned alignment padding
    /// untouched.
    pub(crate) fn copy_row_prefix_from_slice(
        &mut self,
        row: u16,
        bytes: &[u8],
    ) -> Result<(), Jbig2Error> {
        let dst = self
            .get_line_mut(row)
            .and_then(|line| line.get_mut(..bytes.len()))
            .ok_or(Jbig2Error::InvalidState(ROW_WRITE))?;
        dst.copy_from_slice(bytes);
        Ok(())
    }

    /// Read one JBIG2 pixel, returning white for out-of-bounds coordinates.
    ///
    /// Section 7 arithmetic templates and section 8 composition both treat
    /// clipped or unavailable source pixels as white in this decoder.
    pub(crate) fn get_pixel(&self, x: u16, y: u16) -> u8 {
        if x >= self.width || y >= self.height {
            return WHITE_PIXEL;
        }
        let byte_idx = usize::from(x / BITS_PER_BYTE);
        let bit = MSB_FIRST_BIT_INDEX.saturating_sub(x % BITS_PER_BYTE);
        self.get_line(y)
            .and_then(|line| line.get(byte_idx).copied())
            .map_or(WHITE_PIXEL, |byte| (byte >> u32::from(bit)) & BLACK_PIXEL)
    }

    /// Read a pixel relative to base coordinates.
    ///
    /// `x` and `y` are the base coordinates, while `x_offset` and `y_offset`
    /// are applied relative to that base. Out-of-bounds relative positions
    /// return `0`.
    pub(crate) fn pixel_at_offset(&self, x: u16, y: u16, x_offset: i8, y_offset: i8) -> u16 {
        let Some(x) = Self::offset_coord(x, x_offset) else {
            return u16::from(WHITE_PIXEL);
        };
        let Some(y) = Self::offset_coord(y, y_offset) else {
            return u16::from(WHITE_PIXEL);
        };
        u16::from(self.get_pixel(x, y))
    }

    /// Resolve one signed GBAT pair and read that pixel relative to base coordinates.
    ///
    /// `x` and `y` are the base coordinates. `offset` selects one signed
    /// `(x, y)` pair from the normalized GBAT table, and the lookup then uses
    /// the same out-of-bounds-as-zero behavior as [`Self::pixel_at_offset`].
    pub(crate) fn pixel_at_gbat_offset(
        &self,
        x: u16,
        y: u16,
        gbat: GenericRegionAdaptiveTemplate,
        offset: usize,
    ) -> Result<u16, Jbig2Error> {
        let (x_offset, y_offset) = gbat.pair(offset)?;
        Ok(self.pixel_at_offset(x, y, x_offset, y_offset))
    }

    /// Set one JBIG2 pixel to white or black.
    ///
    /// JBIG2 bi-level image procedures store non-zero decoded values as black.
    /// Coordinates outside the bitmap are clipped and ignored.
    pub(crate) fn set_pixel(&mut self, x: u16, y: u16, value: u8) {
        if x >= self.width || y >= self.height {
            return;
        }
        let byte_idx = usize::from(x / BITS_PER_BYTE);
        let bit = MSB_FIRST_BIT_INDEX.saturating_sub(x % BITS_PER_BYTE);
        let mask = 1u8 << u32::from(bit);
        if let Some(line) = self.get_line_mut(y)
            && let Some(byte) = line.get_mut(byte_idx)
        {
            if value != WHITE_PIXEL {
                *byte |= mask;
            } else {
                *byte &= !mask;
            }
        }
    }

    /// Set a pixel only when the decoded value is black.
    ///
    /// JBIG2 generic-region decoding emits `0` for white and non-zero values
    /// for black, so this helper only mutates the bitmap when `pixel != 0`.
    pub(crate) fn set_pixel_if_black(&mut self, x: u16, y: u16, pixel: u8) {
        if pixel != WHITE_PIXEL {
            self.set_pixel(x, y, pixel);
        }
    }

    /// Apply a signed JBIG2 adaptive-template offset to one coordinate.
    fn offset_coord(coord: u16, delta: i8) -> Option<u16> {
        if delta >= 0 {
            coord.checked_add(u16::from(delta.unsigned_abs()))
        } else {
            coord.checked_sub(u16::from(delta.unsigned_abs()))
        }
    }

    /// Return the byte range for one aligned internal row.
    fn row_byte_range(&self, row: u16) -> Option<Range<usize>> {
        if row >= self.height {
            return None;
        }
        BitmapGeometry::row_range(row, self.stride, self.data.len())
    }

    /// Copy one aligned row within the bitmap.
    ///
    /// Generic-region typical-prediction handling in T.88 section 7.4.6 can
    /// duplicate a previous decoded row. If the source row is outside the
    /// image, the destination row is cleared to white.
    pub(crate) fn copy_line(&mut self, to: u16, from: u16) {
        let Some(dst) = self.row_byte_range(to) else {
            return;
        };
        if from >= self.height {
            if let Some(dst_row) = self.data.get_mut(dst) {
                dst_row.fill(EMPTY_BYTE);
            }
            return;
        }
        let Some(src) = self.row_byte_range(from) else {
            return;
        };
        self.data.copy_within(src, dst.start);
    }

    /// Fill the entire aligned bitmap with a JBIG2 default pixel value.
    ///
    /// Page information and region headers in section 7 can define a default
    /// white or black bitmap state before subsequent region composition.
    #[allow(dead_code)]
    pub(crate) fn fill(&mut self, value: bool) {
        self.data.fill(if value { FULL_BYTE } else { EMPTY_BYTE });
    }

    /// XOR this bitmap in place with another JBIG2 image.
    ///
    /// JBIG2 T.88 / ISO/IEC 14492 section 6.6.5 reconstructs each lower
    /// halftone gray plane by XORing it with the next higher plane. This
    /// helper applies that operation directly to the internal 4-byte-aligned
    /// bitmap storage, mutating `self` byte-by-byte.
    pub(crate) fn xor_from(&mut self, other: &JBig2Image) {
        for (dst_byte, src_byte) in self.data.iter_mut().zip(other.data.iter().copied()) {
            *dst_byte ^= src_byte;
        }
    }

    /// Extract a checked JBIG2 subimage.
    ///
    /// Symbol and pattern dictionaries in T.88 sections 7.4.2 and 7.4.4 store
    /// multiple images in a collective bitmap. This helper copies the selected
    /// rectangle into a standalone bitmap and propagates allocation failures.
    pub(crate) fn try_sub_image(
        &self,
        x: u16,
        y: u16,
        width: u16,
        height: u16,
    ) -> Result<JBig2Image, Jbig2Error> {
        let mut image = JBig2Image::try_new(width, height, None)?;
        if self.copy_aligned_sub_image_to(x, y, &mut image)? {
            return Ok(image);
        }

        for row in 0..height {
            for col in 0..width {
                let Some(src_x) = x.checked_add(col) else {
                    continue;
                };
                let Some(src_y) = y.checked_add(row) else {
                    continue;
                };
                let value = self.get_pixel(src_x, src_y);
                if value != 0 {
                    image.set_pixel(col, row, value);
                }
            }
        }
        Ok(image)
    }

    /// Copy a byte-aligned subimage into `dst`.
    ///
    /// Collective bitmaps often store symbols and patterns on byte boundaries.
    /// This path copies whole row byte spans and clears unused tail bits in the
    /// destination row, avoiding per-pixel reads for that common case.
    fn copy_aligned_sub_image_to(
        &self,
        x: u16,
        y: u16,
        dst: &mut JBig2Image,
    ) -> Result<bool, Jbig2Error> {
        if x & BYTE_ALIGNMENT_MASK != 0 {
            return Ok(false);
        }

        let row_bytes = usize::from(dst.row_bytes());
        if row_bytes == 0 {
            return Ok(true);
        }

        let src_byte = usize::from(x / BITS_PER_BYTE);
        for row in 0..dst.height {
            let Some(src_row_index) = y.checked_add(row) else {
                return Ok(false);
            };
            let Some(src_row) = self.get_line(src_row_index) else {
                return Ok(false);
            };
            let src_range = BitmapGeometry::checked_range(src_byte, row_bytes)?;
            let Some(src_bytes) = src_row.get(src_range) else {
                return Ok(false);
            };
            dst.copy_row_prefix_from_slice(row, src_bytes)?;
            dst.mask_unused_tail_bits(row);
        }
        Ok(true)
    }

    /// Decode an uncompressed collective bitmap from a JBIG2 symbol dictionary.
    ///
    /// T.88 / ISO/IEC 14492 section 7.4.2 can carry Huffman-coded symbol
    /// bitmaps as byte-packed collective bitmap data. The source rows are
    /// tight, while `JBig2Image` stores rows 4-byte aligned internally. The
    /// `BitReader` must already be byte-aligned before this call.
    pub(crate) fn decode_uncompressed_collective_bitmap(
        width: u16,
        height: u16,
        stream: &mut BitReader<'_>,
    ) -> Result<Self, Jbig2Error> {
        let row_bytes = packed_row_len(width)?;
        let byte_len = row_bytes
            .checked_mul(usize::from(height))
            .ok_or(Jbig2Error::Overflow(IMAGE_DIMENSIONS_OVERFLOW))?;
        let src = stream
            .take_from_byte_len(byte_len)
            .ok_or(Jbig2Error::Truncated(COLLECTIVE_BITMAP))?;

        let mut image = JBig2Image::try_new(width, height, None)?;
        let stride = usize::from(image.stride());
        for row in 0..height {
            let row = usize::from(row);
            let src_start = row
                .checked_mul(row_bytes)
                .ok_or(Jbig2Error::Overflow(IMAGE_DIMENSIONS_OVERFLOW))?;
            let dst_start = row
                .checked_mul(stride)
                .ok_or(Jbig2Error::Overflow(IMAGE_DIMENSIONS_OVERFLOW))?;
            let src_row = src
                .get(BitmapGeometry::checked_range(src_start, row_bytes)?)
                .ok_or(Jbig2Error::Truncated(COLLECTIVE_BITMAP))?;
            let dst_row = image
                .data_mut()
                .get_mut(BitmapGeometry::checked_range(dst_start, row_bytes)?)
                .ok_or(Jbig2Error::Truncated(COLLECTIVE_BITMAP))?;
            dst_row.copy_from_slice(src_row);
        }
        Ok(image)
    }

    /// Draw this image into `dst`, clipping any portion that lies outside the destination.
    ///
    /// JBIG2 T.88 / ISO/IEC 14492 section 8.2, "Page image composition",
    /// defines how a region bitmap is combined into the page image with the
    /// region's composition operator. The `x` and `y` placement coordinates
    /// are signed because text and halftone regions may start before the
    /// destination origin; this helper clips the source and destination spans
    /// so only the overlapping area is composited.
    pub(crate) fn compose_clipped_to(&self, dst: &mut JBig2Image, x: i32, y: i32, op: ComposeOp) {
        let Some(rect) = ClippedImageRect::from_signed_origin(self, dst, x, y) else {
            return;
        };
        self.compose_rect_to(dst, rect, op);
    }

    /// Compose a pre-clipped rectangle into another JBIG2 bitmap.
    ///
    /// T.88 / ISO/IEC 14492 section 8.2 defines composition in terms of a
    /// source pixel, destination pixel, and region composition operator.
    fn compose_rect_to(&self, dst: &mut JBig2Image, rect: ClippedImageRect, op: ComposeOp) {
        if self.compose_aligned_rect_to(dst, rect, op) {
            return;
        }

        for row in 0..rect.height {
            for col in 0..rect.width {
                let Some(src_x) = rect.src_x.checked_add(col) else {
                    continue;
                };
                let Some(src_y) = rect.src_y.checked_add(row) else {
                    continue;
                };
                let Some(dst_x) = rect.dst_x.checked_add(col) else {
                    continue;
                };
                let Some(dst_y) = rect.dst_y.checked_add(row) else {
                    continue;
                };
                let src_value = self.get_pixel(src_x, src_y);
                let dst_value = dst.get_pixel(dst_x, dst_y);
                dst.set_pixel(dst_x, dst_y, op.apply(dst_value, src_value));
            }
        }
    }

    /// Compose a rectangle whose source and destination bit offsets match.
    fn compose_aligned_rect_to(
        &self,
        dst: &mut JBig2Image,
        rect: ClippedImageRect,
        op: ComposeOp,
    ) -> bool {
        if rect.src_x & BYTE_ALIGNMENT_MASK != rect.dst_x & BYTE_ALIGNMENT_MASK {
            return false;
        }

        let bit_offset = rect.src_x & BYTE_ALIGNMENT_MASK;
        let Some(span_bytes) = Self::span_bytes(bit_offset, rect.width) else {
            return false;
        };
        let src_start = usize::from(rect.src_x / BITS_PER_BYTE);
        let dst_start = usize::from(rect.dst_x / BITS_PER_BYTE);

        for row in 0..rect.height {
            let Some(src_y) = rect.src_y.checked_add(row) else {
                return false;
            };
            let Some(dst_y) = rect.dst_y.checked_add(row) else {
                return false;
            };
            let Some(src_row) = self.get_line(src_y) else {
                return false;
            };
            let Some(dst_row) = dst.get_line_mut(dst_y) else {
                return false;
            };
            let Ok(src_range) = BitmapGeometry::checked_range(src_start, span_bytes) else {
                return false;
            };
            let Ok(dst_range) = BitmapGeometry::checked_range(dst_start, span_bytes) else {
                return false;
            };
            let Some(src_bytes) = src_row.get(src_range) else {
                return false;
            };
            let Some(dst_bytes) = dst_row.get_mut(dst_range) else {
                return false;
            };

            for (byte_index, (dst_byte, src_byte)) in dst_bytes
                .iter_mut()
                .zip(src_bytes.iter().copied())
                .enumerate()
            {
                let mask = Self::byte_mask_for_span(bit_offset, rect.width, byte_index);
                let composed = op.apply_byte(*dst_byte, src_byte);
                *dst_byte = (*dst_byte & !mask) | (composed & mask);
            }
        }
        true
    }

    /// Return tightly packed JBIG2 row bytes without internal alignment padding.
    ///
    /// The PDF `JBIG2Decode` filter expects section 7 bitmap data as compact
    /// MSB-first rows. This performs one output allocation sized to the exact
    /// tight row length times height.
    #[allow(dead_code)]
    pub(crate) fn to_tight_bytes(&self) -> Vec<u8> {
        let row_bytes = usize::from(self.row_bytes());
        if row_bytes == 0 || self.height == 0 {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(row_bytes.saturating_mul(usize::from(self.height)));
        for row in 0..self.height {
            if let Some(line) = self.get_line(row) {
                let end = row_bytes.min(line.len());
                if let Some(part) = line.get(..end) {
                    out.extend_from_slice(part);
                }
            }
        }
        out
    }

    /// Return tightly packed row bytes with every bit inverted.
    ///
    /// The PDF image pipeline expects the opposite polarity from the internal
    /// JBIG2 page bitmap, so decoded page output is inverted after section 8.2
    /// page composition.
    pub(crate) fn inverted_tight_bytes(&self) -> Vec<u8> {
        let row_bytes = usize::from(self.row_bytes());
        if row_bytes == 0 || self.height == 0 {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(row_bytes.saturating_mul(usize::from(self.height)));
        for row in 0..self.height {
            if let Some(line) = self.get_line(row) {
                let end = row_bytes.min(line.len());
                if let Some(part) = line.get(..end) {
                    out.extend(part.iter().map(|byte| !byte));
                }
            }
        }
        out
    }

    /// Return the byte count touched by a bit span.
    fn span_bytes(bit_offset: u16, width: u16) -> Option<usize> {
        let end = usize::from(bit_offset).checked_add(usize::from(width))?;
        end.checked_add(usize::from(BYTE_ALIGNMENT_MASK))
            .and_then(|value| value.checked_div(usize::from(BITS_PER_BYTE)))
    }

    /// Return the mask of significant bits for one byte in a bit span.
    fn byte_mask_for_span(bit_offset: u16, width: u16, byte_index: usize) -> u8 {
        let span_start = usize::from(bit_offset);
        let span_end = span_start.saturating_add(usize::from(width));
        let byte_start = byte_index.saturating_mul(usize::from(BITS_PER_BYTE));
        let byte_end = byte_start.saturating_add(usize::from(BITS_PER_BYTE));
        let start = span_start.max(byte_start);
        let end = span_end.min(byte_end);
        if start >= end {
            return EMPTY_BYTE;
        }

        let mut mask = EMPTY_BYTE;
        for bit in start..end {
            let bit_in_byte = bit.saturating_sub(byte_start);
            if let Ok(bit_in_byte) = u16::try_from(bit_in_byte) {
                let shift = MSB_FIRST_BIT_INDEX.saturating_sub(bit_in_byte);
                mask |= 1u8 << u32::from(shift);
            }
        }
        mask
    }

    /// Clear unused low-order bits in the final tight byte of a row.
    fn mask_unused_tail_bits(&mut self, row: u16) {
        let significant_tail_bits = self.width % BITS_PER_BYTE;
        if significant_tail_bits == 0 {
            return;
        }
        let last_byte_index = usize::from(self.row_bytes().saturating_sub(1));
        let mask = Self::byte_mask_for_span(0, significant_tail_bits, 0);
        if let Some(line) = self.get_line_mut(row)
            && let Some(byte) = line.get_mut(last_byte_index)
        {
            *byte &= mask;
        }
    }
}

/// Checked storage geometry for one internal JBIG2 bitmap.
///
/// JBIG2 bitmap rows are interpreted as tightly packed pixels, but this
/// decoder stores aligned rows to match the rest of the rendering pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BitmapGeometry {
    width: u16,
    height: u16,
    stride: u16,
    byte_len: usize,
}

impl BitmapGeometry {
    /// Compute checked bitmap storage geometry for JBIG2 section 7 dimensions.
    fn new(width: u16, height: u16) -> Result<Self, Jbig2Error> {
        if !JBig2Image::is_valid_image_size(width, height) {
            return Ok(Self {
                width: 0,
                height: 0,
                stride: 0,
                byte_len: 0,
            });
        }

        let Some(stride) = u32::from(width)
            .checked_add(ROW_ALIGNMENT_ROUNDING_BITS)
            .and_then(|value| value.checked_div(ROW_ALIGNMENT_BITS))
            .and_then(|value| value.checked_mul(ROW_ALIGNMENT_BYTES))
            .and_then(|value| u16::try_from(value).ok())
        else {
            return Err(Jbig2Error::Overflow(IMAGE_DIMENSIONS_OVERFLOW));
        };

        let Some(byte_len) = usize::from(stride).checked_mul(usize::from(height)) else {
            return Err(Jbig2Error::Overflow(IMAGE_DIMENSIONS_OVERFLOW));
        };

        Ok(Self {
            width,
            height,
            stride,
            byte_len,
        })
    }

    /// Return whether this geometry contains no JBIG2 pixels.
    fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Compute the tight MSB-first row length for a JBIG2 bitmap width.
    fn tight_row_bytes(width: u16) -> Result<u16, Jbig2Error> {
        let row_bytes = packed_row_len(width)?;
        u16::try_from(row_bytes).map_err(|_| Jbig2Error::Overflow(IMAGE_DIMENSIONS_OVERFLOW))
    }

    /// Return a checked byte range beginning at `start` with `len` bytes.
    fn checked_range(start: usize, len: usize) -> Result<Range<usize>, Jbig2Error> {
        let end = start
            .checked_add(len)
            .ok_or(Jbig2Error::Overflow(IMAGE_DIMENSIONS_OVERFLOW))?;
        Ok(start..end)
    }

    /// Return the checked aligned row range inside `storage_len`.
    fn row_range(row: u16, stride: u16, storage_len: usize) -> Option<Range<usize>> {
        let start = usize::from(row).checked_mul(usize::from(stride))?;
        let range = Self::checked_range(start, usize::from(stride)).ok()?;
        if range.end > storage_len {
            return None;
        }
        Some(range)
    }
}

/// A source and destination rectangle clipped for JBIG2 image composition.
///
/// T.88 / ISO/IEC 14492 section 8.2 composes only the overlapping part of a
/// region bitmap and the page image when placement falls outside the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ClippedImageRect {
    src_x: u16,
    src_y: u16,
    dst_x: u16,
    dst_y: u16,
    width: u16,
    height: u16,
}

impl ClippedImageRect {
    /// Clip a source image placed at a signed JBIG2 page origin.
    fn from_signed_origin(src: &JBig2Image, dst: &JBig2Image, x: i32, y: i32) -> Option<Self> {
        let src_x = if x < 0 {
            u16::try_from(x.unsigned_abs()).ok()?
        } else {
            0
        };
        let src_y = if y < 0 {
            u16::try_from(y.unsigned_abs()).ok()?
        } else {
            0
        };
        let dst_x = if x < 0 { 0 } else { u16::try_from(x).ok()? };
        let dst_y = if y < 0 { 0 } else { u16::try_from(y).ok()? };
        Self::from_unsigned_rect(
            src,
            dst,
            dst_x,
            dst_y,
            src_x,
            src_y,
            src.width.saturating_sub(src_x),
            src.height.saturating_sub(src_y),
        )
    }

    /// Clip an unsigned source rectangle against the destination bitmap.
    ///
    /// This models the bounded portion of T.88 section 8.2 page composition
    /// after any signed origin has been normalized.
    #[allow(clippy::too_many_arguments)]
    fn from_unsigned_rect(
        src: &JBig2Image,
        dst: &JBig2Image,
        dst_x: u16,
        dst_y: u16,
        src_x: u16,
        src_y: u16,
        width: u16,
        height: u16,
    ) -> Option<Self> {
        if src_x >= src.width || src_y >= src.height || dst_x >= dst.width || dst_y >= dst.height {
            return None;
        }
        let width =
            min(width, src.width.saturating_sub(src_x)).min(dst.width.saturating_sub(dst_x));
        let height =
            min(height, src.height.saturating_sub(src_y)).min(dst.height.saturating_sub(dst_y));
        if width == 0 || height == 0 {
            return None;
        }
        Some(Self {
            src_x,
            src_y,
            dst_x,
            dst_y,
            width,
            height,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{BitmapGeometry, ClippedImageRect, ComposeOp, JBig2Image};
    use crate::error::Jbig2Error;
    use crate::generic_region::{GenericRegionAdaptiveTemplate, GenericRegionTemplate};
    use pdf_utils::BitReader;

    fn lit_pixels(image: &JBig2Image) -> Vec<(u16, u16)> {
        let mut pixels = Vec::new();
        for y in 0..image.height() {
            for x in 0..image.width() {
                if image.get_pixel(x, y) != super::WHITE_PIXEL {
                    pixels.push((x, y));
                }
            }
        }
        pixels
    }

    #[test]
    fn create_and_set_pixels() {
        let mut img = JBig2Image::new(80, 20);
        assert_eq!(img.width(), 80);
        assert_eq!(img.height(), 20);
        assert_eq!(img.stride(), 12);
        assert_eq!(img.row_bytes(), 10);
        assert_eq!(img.get_pixel(0, 0), 0);
        img.set_pixel(0, 0, 1);
        img.set_pixel(79, 19, 1);
        assert_eq!(img.get_pixel(0, 0), 1);
        assert_eq!(img.get_pixel(79, 19), 1);
        assert_eq!(
            img.get_line(0).and_then(|line| line.first().copied()),
            Some(0x80)
        );
    }

    #[test]
    fn read_row_byte_reads_stored_byte_and_reports_missing_offsets() {
        let mut img = JBig2Image::new(8, 1);
        if let Some(byte) = img.data_mut().get_mut(0) {
            *byte = 0xa5;
        }

        assert_eq!(img.read_row_byte(0, 0), Ok(0xa5));
        assert_eq!(
            img.read_row_byte(1, 0),
            Err(Jbig2Error::InvalidState("row reference"))
        );
        assert_eq!(
            img.read_row_byte(0, usize::from(img.stride())),
            Err(Jbig2Error::InvalidState("row reference"))
        );
    }

    #[test]
    fn zero_sized_images_remain_empty() {
        let img = JBig2Image::new(0, 7);

        assert_eq!(img.width(), 0);
        assert_eq!(img.height(), 0);
        assert_eq!(img.stride(), 0);
        assert_eq!(img.row_bytes(), 0);
        assert!(img.data().is_empty());
    }

    #[test]
    fn try_new_can_seed_all_storage_bytes() {
        let black = JBig2Image::try_new(9, 2, Some(true)).expect("black bitmap");
        assert!(
            black
                .data()
                .iter()
                .copied()
                .all(|byte| byte == super::FULL_BYTE)
        );

        let white = JBig2Image::try_new(9, 2, Some(false)).expect("white bitmap");
        assert!(
            white
                .data()
                .iter()
                .copied()
                .all(|byte| byte == super::EMPTY_BYTE)
        );
    }

    #[test]
    fn copy_line_and_sub_image_use_u16_coordinates() {
        let mut img = JBig2Image::new(8, 2);
        img.set_pixel(0, 0, 1);
        img.copy_line(1, 0);

        assert_eq!(img.get_pixel(0, 1), 1);

        let sub = img.try_sub_image(0, 1, 4, 1).expect("subimage");
        assert_eq!(sub.width(), 4);
        assert_eq!(sub.height(), 1);
        assert_eq!(sub.get_pixel(0, 0), 1);
    }

    #[test]
    fn bitmap_geometry_checks_aligned_ranges() {
        let geometry = BitmapGeometry::new(65, 2).expect("geometry");

        assert_eq!(geometry.stride, 12);
        assert_eq!(geometry.byte_len, 24);
        assert_eq!(
            BitmapGeometry::row_range(1, geometry.stride, geometry.byte_len),
            Some(12..24)
        );
        assert_eq!(
            BitmapGeometry::row_range(2, geometry.stride, geometry.byte_len),
            None
        );
    }

    #[test]
    fn copy_line_copies_a_valid_row_without_allocation() {
        let mut img = JBig2Image::new(9, 2);
        if let Some(row) = img.get_line_mut(0) {
            row.copy_from_slice(&[0x11, 0x22, 0x33, 0x44]);
        }

        img.copy_line(1, 0);

        assert_eq!(img.get_line(1), Some(&[0x11, 0x22, 0x33, 0x44][..]));
    }

    #[test]
    fn copy_line_to_self_preserves_the_row() {
        let mut img = JBig2Image::new(9, 1);
        if let Some(row) = img.get_line_mut(0) {
            row.copy_from_slice(&[0xaa, 0xbb, 0xcc, 0xdd]);
        }

        img.copy_line(0, 0);

        assert_eq!(img.get_line(0), Some(&[0xaa, 0xbb, 0xcc, 0xdd][..]));
    }

    #[test]
    fn copy_line_from_out_of_bounds_clears_the_destination_row() {
        let mut img = JBig2Image::new(9, 2);
        if let Some(row) = img.get_line_mut(1) {
            row.copy_from_slice(&[0x11, 0x22, 0x33, 0x44]);
        }

        img.copy_line(1, 2);

        assert_eq!(img.get_line(1), Some(&[0x00, 0x00, 0x00, 0x00][..]));
    }

    #[test]
    fn copy_line_to_out_of_bounds_is_ignored() {
        let mut img = JBig2Image::new(9, 1);
        if let Some(row) = img.get_line_mut(0) {
            row.copy_from_slice(&[0x11, 0x22, 0x33, 0x44]);
        }

        img.copy_line(1, 0);

        assert_eq!(img.get_line(0), Some(&[0x11, 0x22, 0x33, 0x44][..]));
        assert_eq!(img.get_line(1), None);
    }

    #[test]
    fn copy_row_prefix_rejects_prefix_longer_than_storage_row() {
        let mut img = JBig2Image::new(8, 1);

        let err = img.copy_row_prefix_from_slice(0, &[0; 5]);

        assert_eq!(err, Err(Jbig2Error::InvalidState("row write")));
    }

    #[test]
    fn xor_from_xors_bitmap_bytes_in_place() {
        let mut image = JBig2Image::new(8, 1);
        image.set_pixel(0, 0, 1);
        image.set_pixel(2, 0, 1);
        image.set_pixel(7, 0, 1);

        let mut other = JBig2Image::new(8, 1);
        other.set_pixel(2, 0, 1);
        other.set_pixel(3, 0, 1);
        other.set_pixel(7, 0, 1);

        image.xor_from(&other);

        assert_eq!(lit_pixels(&image), vec![(0, 0), (3, 0)]);
    }

    #[test]
    fn pixel_at_offset_returns_zero_for_out_of_bounds_relative_positions() {
        let mut img = JBig2Image::new(2, 2);
        img.set_pixel(1, 1, 1);

        assert_eq!(img.pixel_at_offset(0, 1, -1, 0), 0u16);
        assert_eq!(img.pixel_at_offset(1, 0, 0, -1), 0u16);
        assert_eq!(img.pixel_at_offset(1, 1, 1, 0), 0u16);
        assert_eq!(img.pixel_at_offset(1, 1, 0, 1), 0u16);
    }

    #[test]
    fn pixel_at_offset_returns_in_bounds_pixel() {
        let mut img = JBig2Image::new(3, 3);
        img.set_pixel(2, 1, 1);

        assert_eq!(img.pixel_at_offset(1, 1, 1, 0), 1u16);
    }

    #[test]
    fn pixel_at_gbat_offset_resolves_signed_pair() {
        let template = GenericRegionAdaptiveTemplate::from(
            &[0x01, 0xff],
            0,
            false,
            GenericRegionTemplate::Template2,
        )
        .expect("template");
        let mut img = JBig2Image::new(4, 4);
        img.set_pixel(2, 0, 1);

        assert_eq!(img.pixel_at_gbat_offset(1, 1, template, 0), Ok(1u16));
    }

    #[test]
    fn pixel_at_gbat_offset_propagates_invalid_offset() {
        let template = GenericRegionAdaptiveTemplate::from(
            &[0x01, 0xff],
            0,
            false,
            GenericRegionTemplate::Template2,
        )
        .expect("template");
        let img = JBig2Image::new(4, 4);

        assert!(matches!(
            img.pixel_at_gbat_offset(1, 1, template, 8),
            Err(crate::error::Jbig2Error::InvalidTable("adaptive template"))
        ));
    }

    #[test]
    fn set_pixel_if_black_ignores_white_pixels() {
        let mut img = JBig2Image::new(2, 2);

        img.set_pixel_if_black(1, 1, 0);

        assert_eq!(img.get_pixel(1, 1), 0);
    }

    #[test]
    fn set_pixel_if_black_sets_black_pixels() {
        let mut img = JBig2Image::new(2, 2);

        img.set_pixel_if_black(1, 1, 1);

        assert_eq!(img.get_pixel(1, 1), 1);
    }

    #[test]
    fn decode_uncompressed_collective_bitmap_decodes_and_advances_reader() {
        let data = [0b1010_0000, 0b0100_0000, 0b1111_0000, 0b0001_0000, 0xaa];
        let mut reader = BitReader::new(&data);

        let image = JBig2Image::decode_uncompressed_collective_bitmap(10, 2, &mut reader)
            .expect("collective bitmap");
        assert_eq!(image.width(), 10);
        assert_eq!(image.height(), 2);
        assert_eq!(reader.byte_pos(), 4);
        assert_eq!(
            lit_pixels(&image),
            vec![(0, 0), (2, 0), (9, 0), (0, 1), (1, 1), (2, 1), (3, 1)]
        );
    }

    #[test]
    fn decode_uncompressed_collective_bitmap_reports_truncation() {
        let data = [0b1010_0000, 0b0100_0000, 0b1111_0000];
        let mut reader = BitReader::new(&data);

        let err = JBig2Image::decode_uncompressed_collective_bitmap(10, 2, &mut reader);

        assert_eq!(err, Err(Jbig2Error::Truncated("collective bitmap")));
        assert_eq!(reader.byte_pos(), 0);
    }

    #[test]
    fn valid_image_size_accepts_decoder_cap_u16_boundary() {
        assert!(JBig2Image::is_valid_image_size(65_535, 65_535));
        assert!(!JBig2Image::is_valid_image_size(0, 1));
        assert!(!JBig2Image::is_valid_image_size(1, 0));
    }

    #[test]
    fn compose_replace_copies_bits() {
        let mut src = JBig2Image::new(8, 1);
        src.set_pixel(0, 0, 1);
        let mut dst = JBig2Image::new(8, 1);
        let rect = ClippedImageRect::from_unsigned_rect(&src, &dst, 0, 0, 0, 0, 8, 1)
            .expect("composition rectangle");
        src.compose_rect_to(&mut dst, rect, ComposeOp::Replace);
        assert_eq!(dst.get_pixel(0, 0), 1);
    }

    #[test]
    fn compose_rect_applies_all_jbig2_operators() {
        let cases = [
            (ComposeOp::Or, 0, 1, 1),
            (ComposeOp::And, 1, 0, 0),
            (ComposeOp::Xor, 1, 1, 0),
            (ComposeOp::Xnor, 1, 1, 1),
            (ComposeOp::Replace, 1, 0, 0),
        ];

        for (op, dst_pixel, src_pixel, expected) in cases {
            let mut src = JBig2Image::new(1, 1);
            src.set_pixel(0, 0, src_pixel);
            let mut dst = JBig2Image::new(1, 1);
            dst.set_pixel(0, 0, dst_pixel);
            let rect = ClippedImageRect::from_unsigned_rect(&src, &dst, 0, 0, 0, 0, 1, 1)
                .expect("composition rectangle");

            src.compose_rect_to(&mut dst, rect, op);

            assert_eq!(dst.get_pixel(0, 0), expected, "operator {op:?}");
        }
    }

    #[test]
    fn aligned_compose_preserves_bits_outside_rect_edges() {
        let mut src = JBig2Image::new(16, 1);
        src.set_pixel(2, 0, 1);
        src.set_pixel(3, 0, 1);
        src.set_pixel(4, 0, 1);
        src.set_pixel(5, 0, 1);
        let mut dst = JBig2Image::new(16, 1);
        dst.set_pixel(0, 0, 1);
        dst.set_pixel(15, 0, 1);
        let rect = ClippedImageRect::from_unsigned_rect(&src, &dst, 2, 0, 2, 0, 4, 1)
            .expect("composition rectangle");

        src.compose_rect_to(&mut dst, rect, ComposeOp::Replace);

        assert_eq!(
            lit_pixels(&dst),
            vec![(0, 0), (2, 0), (3, 0), (4, 0), (5, 0), (15, 0)]
        );
    }

    #[test]
    fn compose_clipped_to_accepts_negative_coordinates() {
        let mut src = JBig2Image::new(2, 2);
        src.set_pixel(0, 0, 1);
        src.set_pixel(1, 0, 1);
        src.set_pixel(0, 1, 1);
        src.set_pixel(1, 1, 1);
        let mut dst = JBig2Image::new(3, 3);

        src.compose_clipped_to(&mut dst, -1, -1, ComposeOp::Replace);

        assert_eq!(lit_pixels(&dst), vec![(0, 0)]);
    }

    #[test]
    fn compose_clipped_to_accepts_negative_y_only() {
        let mut src = JBig2Image::new(2, 2);
        src.set_pixel(0, 1, 1);
        src.set_pixel(1, 1, 1);
        let mut dst = JBig2Image::new(3, 3);

        src.compose_clipped_to(&mut dst, 1, -1, ComposeOp::Replace);

        assert_eq!(lit_pixels(&dst), vec![(1, 0), (2, 0)]);
    }

    #[test]
    fn tight_rows_are_packed_without_padding() {
        let mut img = JBig2Image::new(13, 2);
        img.set_pixel(0, 0, 1);
        img.set_pixel(12, 1, 1);

        let bytes = img.to_tight_bytes();
        assert_eq!(bytes, vec![0x80, 0x00, 0x00, 0x08]);
    }

    #[test]
    fn byte_aligned_sub_image_masks_tail_bits() {
        let mut img = JBig2Image::new(16, 1);
        img.set_pixel(0, 0, 1);
        img.set_pixel(4, 0, 1);
        img.set_pixel(7, 0, 1);

        let sub = img.try_sub_image(0, 0, 5, 1).expect("subimage");

        assert_eq!(sub.to_tight_bytes(), vec![0x88]);
        assert_eq!(lit_pixels(&sub), vec![(0, 0), (4, 0)]);
    }

    #[test]
    fn inverted_tight_rows_invert_packed_bytes() {
        let mut img = JBig2Image::new(8, 1);
        img.set_pixel(0, 0, 1);
        img.set_pixel(7, 0, 1);

        assert_eq!(img.inverted_tight_bytes(), vec![0x7e]);
    }
}
