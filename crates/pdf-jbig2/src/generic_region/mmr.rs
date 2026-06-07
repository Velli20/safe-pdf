use pdf_ccitt::{CCITTFaxParams, decode_rows as ccitt_decode_rows};

use crate::{error::Jbig2Error, image::JBig2Image, util::packed_row_len};

/// Decode an MMR-coded JBIG2 generic region.
///
/// ITU-T T.88 / ISO/IEC 14492 section 7.4.6.2 selects this path when `MMR = 1`.
/// The compressed body is CCITT Group 4 encoded, and JBIG2's bitmap convention
/// requires inverting the decoded CCITT bytes into `JBig2Image` storage.
pub(crate) fn decode_mmr_region(
    width: u16,
    height: u16,
    data: &[u8],
) -> Result<JBig2Image, Jbig2Error> {
    let row_bytes = packed_row_len(width)?;
    let ccitt_params = CCITTFaxParams {
        k: -1,
        columns: usize::from(width),
        rows: usize::from(height),
        end_of_line: false,
        encoded_byte_align: false,
        end_of_block: true,
        black_is1: false,
        damaged_rows_before_error: 0,
    };

    let mut image = JBig2Image::try_new(width, height, None)?;
    let stride = usize::from(image.stride());
    let image_data_len = image.data().len();
    let mut row_index = 0usize;

    ccitt_decode_rows(data, &ccitt_params, |src_row| {
        let Some(dst_start) = row_index.checked_mul(stride) else {
            return;
        };
        let Some(dst_end) = dst_start.checked_add(row_bytes) else {
            return;
        };
        if dst_end > image_data_len {
            return;
        }
        let Some(dst_row) = image.data_mut().get_mut(dst_start..dst_end) else {
            return;
        };
        for (dst, src) in dst_row.iter_mut().zip(src_row.iter().copied()) {
            *dst = !src;
        }
        row_index = row_index.saturating_add(1);
    })?;

    Ok(image)
}

#[cfg(test)]
mod tests {
    use super::decode_mmr_region;
    use crate::error::Jbig2Error;

    #[test]
    fn mmr_failure_is_wrapped() {
        let err = decode_mmr_region(0, 1, &[]).expect_err("expected error");
        assert!(matches!(err, Jbig2Error::Ccitt(_)));
    }

    #[test]
    fn mmr_streams_into_aligned_image_rows() -> Result<(), Jbig2Error> {
        let image = decode_mmr_region(9, 1, &[0x80])?;

        assert_eq!(image.stride(), 4);
        assert_eq!(image.to_tight_bytes(), [0x00, 0x00]);
        assert_eq!(image.data(), [0x00, 0x00, 0x00, 0x00]);
        Ok(())
    }
}
