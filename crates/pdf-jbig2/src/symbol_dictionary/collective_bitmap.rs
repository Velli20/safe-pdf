use crate::{error::Jbig2Error, image::JBig2Image};

const IMAGE_DIMENSIONS_OVERFLOW: &str = "image dimensions overflow";

/// Split a symbol dictionary collective bitmap into individual symbols.
///
/// ITU-T T.88 / ISO/IEC 14492 section 7.4.2 stores Huffman-coded symbol
/// bitmaps of the same height in a collective bitmap, concatenated
/// horizontally in decoded symbol order.
pub(super) fn append_collective_bitmap_symbols(
    new_symbols: &mut Vec<JBig2Image>,
    collective_bitmap: &JBig2Image,
    widths: &[u16],
    height: u16,
) -> Result<(), Jbig2Error> {
    new_symbols
        .try_reserve(widths.len())
        .map_err(|_| Jbig2Error::Allocation("symbol images"))?;

    let mut x = 0u16;
    for width in widths {
        let image = collective_bitmap.try_sub_image(x, 0, *width, height)?;
        new_symbols.push(image);
        x = x
            .checked_add(*width)
            .ok_or(Jbig2Error::Overflow(IMAGE_DIMENSIONS_OVERFLOW))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::append_collective_bitmap_symbols;
    use crate::image::JBig2Image;

    #[test]
    fn collective_bitmap_is_split_by_symbol_widths() {
        let mut collective = JBig2Image::new(4, 1);
        collective.set_pixel(0, 0, 1);
        collective.set_pixel(2, 0, 1);
        collective.set_pixel(3, 0, 1);
        let mut symbols = Vec::new();

        append_collective_bitmap_symbols(&mut symbols, &collective, &[1, 3], 1).expect("split");

        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols.first().expect("first").width(), 1);
        assert_eq!(symbols.first().expect("first").get_pixel(0, 0), 1);
        assert_eq!(symbols.get(1).expect("second").width(), 3);
        assert_eq!(symbols.get(1).expect("second").get_pixel(0, 0), 0);
        assert_eq!(symbols.get(1).expect("second").get_pixel(1, 0), 1);
        assert_eq!(symbols.get(1).expect("second").get_pixel(2, 0), 1);
    }
}
