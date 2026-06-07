//! JBIG2 text-region symbol placement geometry.

use crate::{
    error::Jbig2Error, text_region::flags::TextRegionFlagBits, util::INTEGER_CONVERSION_OVERFLOW,
};

const BOTTOM_LEFT_CODE: u8 = 0;
const TOP_LEFT_CODE: u8 = 1;
const BOTTOM_RIGHT_CODE: u8 = 2;
const TOP_RIGHT_CODE: u8 = 3;

/// Text-region reference-corner mapping.
///
/// ITU-T T.88 | ISO/IEC 14492 section 7.4.3.1.1 Table 9 defines the encoded
/// `REFCORNER` values used by the section 6.4.5 symbol placement procedure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TextRegionRefCorner {
    /// Reference corner code `0`: bottom-left.
    BottomLeft = 0,
    /// Reference corner code `1`: top-left.
    TopLeft = 1,
    /// Reference corner code `2`: bottom-right.
    BottomRight = 2,
    /// Reference corner code `3`: top-right.
    TopRight = 3,
}

impl TryFrom<u8> for TextRegionRefCorner {
    type Error = Jbig2Error;

    /// Convert an encoded `REFCORNER` value to its semantic corner.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 section 7.4.3.1.1 Table 9 defines the only
    /// valid values as `0..=3`.
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            BOTTOM_LEFT_CODE => Ok(Self::BottomLeft),
            TOP_LEFT_CODE => Ok(Self::TopLeft),
            BOTTOM_RIGHT_CODE => Ok(Self::BottomRight),
            TOP_RIGHT_CODE => Ok(Self::TopRight),
            _ => Err(Jbig2Error::UnsupportedFeature(
                "text-region reference corner",
            )),
        }
    }
}

/// Final bitmap coordinates for one decoded symbol instance.
///
/// ITU-T T.88 | ISO/IEC 14492 section 6.4.5 step 3(c) computes placement from
/// the decoded `(SI, TI)` pair, transposition flag, and reference corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TextRegionPlacement {
    /// Destination x coordinate in the text-region bitmap.
    pub(crate) x: i32,
    /// Destination y coordinate in the text-region bitmap.
    pub(crate) y: i32,
}

/// Text-region axis and reference-corner configuration.
///
/// ITU-T T.88 | ISO/IEC 14492 section 7.4.3.1.1 Table 9 provides
/// `TRANSPOSED` and `REFCORNER`; section 6.4.5 uses them to map `S` and `T`
/// coordinates to image coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TextRegionGeometry {
    transposed: bool,
    refcorner: TextRegionRefCorner,
}

impl TextRegionGeometry {
    /// Build placement geometry from text-region flags.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 section 7.4.3.1.1 Table 9 stores the
    /// `TRANSPOSED` and `REFCORNER` fields used for text-region placement.
    pub(crate) fn from_flags(flags: TextRegionFlagBits) -> Result<Self, Jbig2Error> {
        Ok(Self::new(
            flags.contains(TextRegionFlagBits::TRANSPOSED),
            TextRegionRefCorner::try_from(flags.refcorner())?,
        ))
    }

    /// Build placement geometry from explicit axis and corner values.
    ///
    /// This represents the section 6.4.5 placement parameters after Table 9
    /// flag parsing has already been performed.
    pub(crate) fn new(transposed: bool, refcorner: TextRegionRefCorner) -> Self {
        Self {
            transposed,
            refcorner,
        }
    }

    /// Adjust `CURS` before computing the symbol placement point.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 section 6.4.5 step 3(c)v shifts `CURS`
    /// before placement for right or bottom reference corners.
    pub(crate) fn adjust_curs_before_placement(
        self,
        curs: i64,
        width: u16,
        height: u16,
    ) -> Result<i64, Jbig2Error> {
        let increment = if !self.transposed {
            match self.refcorner {
                TextRegionRefCorner::BottomRight | TextRegionRefCorner::TopRight => {
                    Some(dimension_minus_one(width)?)
                }
                TextRegionRefCorner::BottomLeft | TextRegionRefCorner::TopLeft => None,
            }
        } else {
            match self.refcorner {
                TextRegionRefCorner::BottomLeft | TextRegionRefCorner::BottomRight => {
                    Some(dimension_minus_one(height)?)
                }
                TextRegionRefCorner::TopLeft | TextRegionRefCorner::TopRight => None,
            }
        };

        add_optional(curs, increment)
    }

    /// Compute destination coordinates for `(SI, TI)`.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 section 6.4.5 step 3(c) maps the decoded
    /// symbol-instance coordinates through `TRANSPOSED` and `REFCORNER`.
    pub(crate) fn placement_for(
        self,
        si: i64,
        ti: i64,
        width: u16,
        height: u16,
    ) -> Result<TextRegionPlacement, Jbig2Error> {
        let width_minus_one = dimension_minus_one(width)?;
        let height_minus_one = dimension_minus_one(height)?;

        let (x, y) = if !self.transposed {
            self.untransposed_placement(si, ti, width_minus_one, height_minus_one)?
        } else {
            self.transposed_placement(si, ti, width_minus_one, height_minus_one)?
        };
        Ok(TextRegionPlacement {
            x: i64_to_i32(x)?,
            y: i64_to_i32(y)?,
        })
    }

    /// Compute untransposed destination coordinates.
    ///
    /// This is the `TRANSPOSED = 0` branch of ITU-T T.88 | ISO/IEC 14492
    /// section 6.4.5 step 3(c) placement.
    fn untransposed_placement(
        self,
        si: i64,
        ti: i64,
        width_minus_one: i64,
        height_minus_one: i64,
    ) -> Result<(i64, i64), Jbig2Error> {
        match self.refcorner {
            TextRegionRefCorner::BottomLeft => Ok((si, checked_sub(ti, height_minus_one)?)),
            TextRegionRefCorner::TopLeft => Ok((si, ti)),
            TextRegionRefCorner::BottomRight => Ok((
                checked_sub(si, width_minus_one)?,
                checked_sub(ti, height_minus_one)?,
            )),
            TextRegionRefCorner::TopRight => Ok((checked_sub(si, width_minus_one)?, ti)),
        }
    }

    /// Compute transposed destination coordinates.
    ///
    /// This is the `TRANSPOSED = 1` branch of ITU-T T.88 | ISO/IEC 14492
    /// section 6.4.5 step 3(c) placement.
    fn transposed_placement(
        self,
        si: i64,
        ti: i64,
        width_minus_one: i64,
        height_minus_one: i64,
    ) -> Result<(i64, i64), Jbig2Error> {
        match self.refcorner {
            TextRegionRefCorner::BottomLeft => Ok((ti, checked_sub(si, height_minus_one)?)),
            TextRegionRefCorner::TopLeft => Ok((ti, si)),
            TextRegionRefCorner::BottomRight => Ok((
                checked_sub(ti, width_minus_one)?,
                checked_sub(si, height_minus_one)?,
            )),
            TextRegionRefCorner::TopRight => Ok((checked_sub(ti, width_minus_one)?, si)),
        }
    }

    /// Advance `CURS` after composing a symbol instance.
    ///
    /// ITU-T T.88 | ISO/IEC 14492 section 6.4.5 step 3(c)x advances `CURS`
    /// by the symbol dimension for left or top reference corners.
    pub(crate) fn advance_curs_after_placement(
        self,
        curs: i64,
        width: u16,
        height: u16,
    ) -> Result<i64, Jbig2Error> {
        let increment = if !self.transposed {
            match self.refcorner {
                TextRegionRefCorner::BottomLeft | TextRegionRefCorner::TopLeft => {
                    Some(dimension_minus_one(width)?)
                }
                TextRegionRefCorner::BottomRight | TextRegionRefCorner::TopRight => None,
            }
        } else {
            match self.refcorner {
                TextRegionRefCorner::TopLeft | TextRegionRefCorner::TopRight => {
                    Some(dimension_minus_one(height)?)
                }
                TextRegionRefCorner::BottomLeft | TextRegionRefCorner::BottomRight => None,
            }
        };

        add_optional(curs, increment)
    }
}

/// Add an optional placement increment with overflow checking.
///
/// This supports the conditional `CURS` updates in ITU-T T.88 | ISO/IEC 14492
/// section 6.4.5 steps 3(c)v and 3(c)x.
fn add_optional(value: i64, increment: Option<i64>) -> Result<i64, Jbig2Error> {
    match increment {
        Some(increment) => value
            .checked_add(increment)
            .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW)),
        None => Ok(value),
    }
}

/// Subtract placement values with overflow checking.
///
/// This supports reference-corner coordinate adjustment in ITU-T T.88 |
/// ISO/IEC 14492 section 6.4.5.
fn checked_sub(left: i64, right: i64) -> Result<i64, Jbig2Error> {
    left.checked_sub(right)
        .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))
}

/// Return a symbol dimension minus one as required by section 6.4.5 placement.
fn dimension_minus_one(value: u16) -> Result<i64, Jbig2Error> {
    i64::from(value)
        .checked_sub(1)
        .ok_or(Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))
}

/// Convert a spec-domain placement coordinate to the bitmap API coordinate type.
fn i64_to_i32(value: i64) -> Result<i32, Jbig2Error> {
    i32::try_from(value).map_err(|_| Jbig2Error::Overflow(INTEGER_CONVERSION_OVERFLOW))
}

#[cfg(test)]
mod tests {
    use super::{TextRegionGeometry, TextRegionRefCorner};
    use crate::error::Jbig2Error;

    #[test]
    fn text_region_refcorner_try_from_validates_known_values() {
        assert_eq!(
            TextRegionRefCorner::try_from(0).expect("corner"),
            TextRegionRefCorner::BottomLeft
        );
        assert_eq!(
            TextRegionRefCorner::try_from(1).expect("corner"),
            TextRegionRefCorner::TopLeft
        );
        assert_eq!(
            TextRegionRefCorner::try_from(2).expect("corner"),
            TextRegionRefCorner::BottomRight
        );
        assert_eq!(
            TextRegionRefCorner::try_from(3).expect("corner"),
            TextRegionRefCorner::TopRight
        );
        assert_eq!(
            TextRegionRefCorner::try_from(4).expect_err("invalid"),
            Jbig2Error::UnsupportedFeature("text-region reference corner")
        );
    }

    #[test]
    fn compute_symbol_placement_covers_transposed_and_corner_combinations() {
        let cases = [
            (false, TextRegionRefCorner::BottomLeft, (10, 18)),
            (false, TextRegionRefCorner::TopLeft, (10, 20)),
            (false, TextRegionRefCorner::BottomRight, (7, 18)),
            (false, TextRegionRefCorner::TopRight, (7, 20)),
            (true, TextRegionRefCorner::BottomLeft, (20, 8)),
            (true, TextRegionRefCorner::TopLeft, (20, 10)),
            (true, TextRegionRefCorner::BottomRight, (17, 8)),
            (true, TextRegionRefCorner::TopRight, (17, 10)),
        ];

        for (transposed, corner, expected_xy) in cases {
            let placement = TextRegionGeometry::new(transposed, corner)
                .placement_for(10, 20, 4, 3)
                .expect("placement");
            assert_eq!((placement.x, placement.y), expected_xy);
        }
    }

    #[test]
    fn compute_symbol_placement_preserves_negative_coordinates() {
        let placement = TextRegionGeometry::new(false, TextRegionRefCorner::BottomRight)
            .placement_for(0, 0, 2, 2)
            .expect("placement");

        assert_eq!((placement.x, placement.y), (-1, -1));
    }

    #[test]
    fn adjust_curs_before_placement_matches_spec_rule() {
        let cases = [
            (false, TextRegionRefCorner::BottomLeft, 10),
            (false, TextRegionRefCorner::TopLeft, 10),
            (false, TextRegionRefCorner::BottomRight, 13),
            (false, TextRegionRefCorner::TopRight, 13),
            (true, TextRegionRefCorner::BottomLeft, 12),
            (true, TextRegionRefCorner::TopLeft, 10),
            (true, TextRegionRefCorner::BottomRight, 12),
            (true, TextRegionRefCorner::TopRight, 10),
        ];

        for (transposed, corner, expected) in cases {
            let adjusted = TextRegionGeometry::new(transposed, corner)
                .adjust_curs_before_placement(10, 4, 3)
                .expect("adjusted");
            assert_eq!(adjusted, expected);
        }
    }

    #[test]
    fn advance_curs_after_placement_matches_spec_rule() {
        let cases = [
            (false, TextRegionRefCorner::BottomLeft, 13),
            (false, TextRegionRefCorner::TopLeft, 13),
            (false, TextRegionRefCorner::BottomRight, 10),
            (false, TextRegionRefCorner::TopRight, 10),
            (true, TextRegionRefCorner::BottomLeft, 10),
            (true, TextRegionRefCorner::TopLeft, 12),
            (true, TextRegionRefCorner::BottomRight, 10),
            (true, TextRegionRefCorner::TopRight, 12),
        ];

        for (transposed, corner, expected) in cases {
            let advanced = TextRegionGeometry::new(transposed, corner)
                .advance_curs_after_placement(10, 4, 3)
                .expect("advanced");
            assert_eq!(advanced, expected);
        }
    }
}
