//! Bounded MMR (ITU-T T.6 Group 4) bitmap reconstruction for JBIG2.
//!
//! JBIG2 reuses Group 4 coding for generic regions, pattern-dictionary
//! collective bitmaps, and halftone grayscale bitplanes (T.88 section 6.2.6).
//! Decoding is delegated to `pdf-ccitt` rather than reimplemented here; this
//! module owns only the JBIG2-specific contract: bounded input, a shared bit
//! cursor across consecutive bitmaps, inverted polarity, and budgeting.

use pdf_ccitt::{CCITTFaxParams, decode_rows_from_reader};
use pdf_utils::BitReader;

use crate::{
    Jbig2Error, Result,
    bitmap::Bitmap,
    document::DecodeBudget,
    error::{LimitKind, SourceLocation},
};

/// Decode one MMR bitmap from the start of `input`, returning the bitmap and
/// the number of bytes consumed.
pub(crate) fn decode(
    width: u32,
    height: u32,
    input: &[u8],
    location: SourceLocation,
    budget: &mut DecodeBudget,
) -> Result<(Bitmap, usize)> {
    let (bitmap, end_bit) = decode_from_bit(width, height, input, 0, location, budget)?;
    Ok((bitmap, end_bit.div_ceil(8)))
}

/// Decode one MMR bitmap starting at an absolute bit position, returning the
/// bitmap and the bit position that follows it.
///
/// Halftone grayscale bitplanes share one payload, so each plane resumes where
/// the previous one's end-of-block marker left the cursor.
pub(crate) fn decode_from_bit(
    width: u32,
    height: u32,
    input: &[u8],
    bit_position: usize,
    location: SourceLocation,
    budget: &mut DecodeBudget,
) -> Result<(Bitmap, usize)> {
    if width == 0 || height == 0 {
        return Err(Jbig2Error::InvalidBitmap("MMR dimensions must be positive"));
    }
    let params = CCITTFaxParams {
        k: -1,
        columns: usize::try_from(width).map_err(|_| Jbig2Error::Overflow("MMR width"))?,
        rows: usize::try_from(height).map_err(|_| Jbig2Error::Overflow("MMR height"))?,
        end_of_line: false,
        encoded_byte_align: false,
        end_of_block: true,
        black_is1: false,
        damaged_rows_before_error: 0,
    };
    budget.charge(
        LimitKind::Work,
        u64::from(width)
            .checked_mul(u64::from(height))
            .ok_or(Jbig2Error::Overflow("MMR work"))?,
    )?;

    let mut bitmap = Bitmap::new(width, height, false, budget)?;
    let mut reader = BitReader::new(input);
    reader.set_pos(bit_position);
    let mut rows = 0_u32;
    // The row sink cannot fail the surrounding call directly, so the first
    // failure is captured and reported once decoding returns.
    let mut failure: Option<Jbig2Error> = None;
    decode_rows_from_reader(&mut reader, &params, |row| {
        if failure.is_some() {
            return;
        }
        if let Err(error) = store_row(&mut bitmap, rows, row) {
            failure = Some(error);
            return;
        }
        rows = rows.saturating_add(1);
    })
    .map_err(|_| Jbig2Error::InvalidCodingData {
        location,
        reason: "MMR coded data",
    })?;
    if let Some(error) = failure {
        return Err(error);
    }
    if rows != height {
        return Err(Jbig2Error::Truncated {
            location,
            needed: 1,
        });
    }
    Ok((bitmap, reader.pos()))
}

/// Store one decoded CCITT row, inverting it into JBIG2's convention that a set
/// bit is black.
fn store_row(bitmap: &mut Bitmap, y: u32, row: &[u8]) -> Result<()> {
    let mut inverted = [0_u8; INVERT_CHUNK];
    let stride = bitmap.as_ref().stride();
    let source = row
        .get(..stride)
        .ok_or(Jbig2Error::InvalidBitmap("MMR row is too short"))?;
    if stride <= INVERT_CHUNK {
        for (destination, byte) in inverted.iter_mut().zip(source.iter().copied()) {
            *destination = !byte;
        }
        return bitmap.write_row(
            y,
            inverted
                .get(..stride)
                .ok_or(Jbig2Error::InvalidBitmap("MMR row buffer"))?,
        );
    }
    let mut owned = Vec::new();
    owned.try_reserve_exact(stride)?;
    owned.extend(source.iter().map(|byte| !byte));
    bitmap.write_row(y, &owned)
}

/// Rows at most this wide are inverted through a stack buffer, which covers
/// every symbol, pattern, and halftone-grid bitmap without allocating.
const INVERT_CHUNK: usize = 256;
