//! JBIG2 generic-region adaptive template (`GBAT`) parsing.
//!
//! ITU-T T.88 / ISO/IEC 14492 section 6.2.5.4 defines adaptive template
//! pixels for arithmetic-coded generic regions, and section 7.4.6.2 defines
//! when those `GBAT` fields are present in a generic-region segment.
//!
//! This module normalizes the wire representation into one internal coordinate
//! table so the arithmetic decoder does not need to distinguish segment syntax
//! variants while decoding pixels.

use crate::error::Jbig2Error;
use pdf_utils::BitReader;

use super::GenericRegionTemplate;

const NORMALIZED_GBAT_LEN: usize = 8;
const TEMPLATE0_GBAT_BYTE_COUNT: usize = 8;
const TEMPLATE123_GBAT_BYTE_COUNT: usize = 2;
#[cfg(test)]
const GBAT_PAIR_LEN: usize = 2;

const TEMPLATE0_DEFAULT_GBAT: [i8; NORMALIZED_GBAT_LEN] = [3, -1, -3, -1, 2, -2, -2, -2];
const TEMPLATE1_DEFAULT_GBAT: [i8; NORMALIZED_GBAT_LEN] = [3, -1, -3, -1, 2, -2, 0, 0];
const TEMPLATE23_DEFAULT_GBAT: [i8; NORMALIZED_GBAT_LEN] = [2, -1, -3, -1, 2, -2, 0, 0];
const TEMPLATE23_OPT3_DEFAULT_GBAT: [i8; TEMPLATE123_GBAT_BYTE_COUNT] = [2, -1];

/// Normalized JBIG2 generic-region adaptive template data.
///
/// ITU-T T.88 / ISO/IEC 14492 section 6.2.5.4 names the signed adaptive
/// template offsets `GBATX1`/`GBATY1` through `GBATX4`/`GBATY4`. Template 0
/// carries all four pairs, while templates 1 through 3 carry only the first
/// pair and use specification defaults for the remaining entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GenericRegionAdaptiveTemplate {
    gbat: [i8; NORMALIZED_GBAT_LEN],
    encoded_len: usize,
}

impl GenericRegionAdaptiveTemplate {
    /// Construct a normalized adaptive-template table directly.
    ///
    /// This is used by higher-level JBIG2 procedures that reuse the generic-
    /// region arithmetic decoder without carrying a generic-region segment
    /// header, as allowed by the arithmetic procedure in section 6.2.5.7.
    pub(crate) const fn from_normalized(gbat: [i8; NORMALIZED_GBAT_LEN]) -> Self {
        Self {
            gbat,
            encoded_len: 0,
        }
    }

    /// Parse and normalize generic-region adaptive-template bytes from `data`.
    ///
    /// `at_flags_offset` points to the first AT byte after the generic-region
    /// flags from section 7.4.6.2. When `MMR = 1`, no AT bytes are consumed.
    /// Otherwise template 0 consumes eight signed bytes, and templates 1
    /// through 3 consume two signed bytes for the first adaptive coordinate
    /// pair from section 6.2.5.4.
    pub(crate) fn from(
        data: &[u8],
        at_flags_offset: usize,
        mmr: bool,
        template: GenericRegionTemplate,
    ) -> Result<Self, Jbig2Error> {
        let Some(data) = data.get(at_flags_offset..) else {
            return Err(Jbig2Error::Truncated("generic adaptive template"));
        };
        let mut reader = BitReader::new(data);
        Self::parse(&mut reader, mmr, template)
    }

    /// Parse and normalize generic-region adaptive-template bytes from `stream`.
    ///
    /// The stream must be positioned immediately after the generic-region flags
    /// from section 7.4.6.2. This method leaves the stream at the start of the
    /// generic-region body described by section 7.4.6.1.
    pub(crate) fn parse(
        stream: &mut BitReader<'_>,
        mmr: bool,
        template: GenericRegionTemplate,
    ) -> Result<Self, Jbig2Error> {
        let mut gbat = template.default_gbat();
        if mmr {
            return Ok(Self {
                gbat,
                encoded_len: 0,
            });
        }

        let encoded_len = template.gbat_byte_count();
        for dst in gbat.iter_mut().take(encoded_len) {
            *dst = stream
                .try_read_i8()
                .map_err(|_| Jbig2Error::Truncated("generic adaptive template"))?;
        }

        Ok(Self { gbat, encoded_len })
    }

    /// Return the normalized eight-entry adaptive template from section 6.2.5.4.
    pub(crate) const fn normalized(self) -> [i8; NORMALIZED_GBAT_LEN] {
        self.gbat
    }

    /// Return the number of AT bytes consumed from the generic-region header.
    #[cfg(test)]
    pub(crate) const fn encoded_len(self) -> usize {
        self.encoded_len
    }

    /// Return whether template 0 uses the default optimized adaptive offsets.
    ///
    /// This identifies the byte-oriented fast path for section 6.2.5.7 when
    /// the section 6.2.5.4 adaptive pixels match the default template-0 shape.
    pub(crate) fn is_template0_opt3_default(self) -> bool {
        self.normalized() == TEMPLATE0_DEFAULT_GBAT
    }

    /// Return whether template 2 or 3 uses the default optimized first pair.
    ///
    /// The optimized decoder only needs the first adaptive pair for the
    /// supported template-2 path; section 6.2.5.4 supplies the remaining
    /// default entries through the normalized table.
    pub(crate) fn uses_template23_opt3(self) -> bool {
        let gbat = self.normalized();
        gbat.first() == TEMPLATE23_OPT3_DEFAULT_GBAT.first()
            && gbat.get(1) == TEMPLATE23_OPT3_DEFAULT_GBAT.get(1)
    }

    /// Return a signed adaptive-template coordinate pair at `offset`.
    ///
    /// Callers pass the even index of the `GBATXn` entry from the normalized
    /// section 6.2.5.4 table; this method returns the adjacent `(x, y)` pair.
    pub(crate) fn pair(self, offset: usize) -> Result<(i8, i8), Jbig2Error> {
        let x_delta = *self
            .gbat
            .get(offset)
            .ok_or(Jbig2Error::InvalidTable("adaptive template"))?;
        let y_delta = *self
            .gbat
            .get(offset.saturating_add(1))
            .ok_or(Jbig2Error::InvalidTable("adaptive template"))?;
        Ok((x_delta, y_delta))
    }
}

impl GenericRegionTemplate {
    /// Return the number of signed `GBAT` bytes encoded by this template.
    ///
    /// ITU-T T.88 / ISO/IEC 14492 section 7.4.6.2 encodes four coordinate
    /// pairs for template 0 and one coordinate pair for templates 1 through 3.
    pub(crate) const fn gbat_byte_count(self) -> usize {
        match self {
            Self::Template0 => TEMPLATE0_GBAT_BYTE_COUNT,
            Self::Template1 | Self::Template2 | Self::Template3 => TEMPLATE123_GBAT_BYTE_COUNT,
        }
    }

    /// Return the normalized default `GBAT` table for this generic template.
    ///
    /// Defaults are the adaptive-template coordinates from section 6.2.5.4,
    /// expanded to the decoder's internal eight-entry representation.
    const fn default_gbat(self) -> [i8; NORMALIZED_GBAT_LEN] {
        match self {
            Self::Template0 => TEMPLATE0_DEFAULT_GBAT,
            Self::Template1 => TEMPLATE1_DEFAULT_GBAT,
            Self::Template2 | Self::Template3 => TEMPLATE23_DEFAULT_GBAT,
        }
    }
}

/// Return the byte offset of the next adaptive-template coordinate pair.
///
/// This tiny helper gives tests a named value for the two-byte `(GBATX, GBATY)`
/// stride described by ITU-T T.88 / ISO/IEC 14492 section 6.2.5.4.
#[cfg(test)]
const fn next_gbat_pair_offset(offset: usize) -> usize {
    offset.saturating_add(GBAT_PAIR_LEN)
}

#[cfg(test)]
mod tests {
    use super::{GenericRegionAdaptiveTemplate, next_gbat_pair_offset};
    use crate::{error::Jbig2Error, generic_region::GenericRegionTemplate};

    #[test]
    fn mmr_uses_normalized_defaults_and_consumes_no_bytes() {
        let template =
            GenericRegionAdaptiveTemplate::from(&[], 0, true, GenericRegionTemplate::Template0)
                .expect("parse");
        assert_eq!(template.normalized(), [3, -1, -3, -1, 2, -2, -2, -2]);
        assert_eq!(template.encoded_len(), 0);
    }

    #[test]
    fn template0_reads_eight_signed_bytes() {
        let data = [0xff, 0x7f, 0x80, 0x00, 0x01, 0xfe, 0x11, 0xee];
        let template =
            GenericRegionAdaptiveTemplate::from(&data, 0, false, GenericRegionTemplate::Template0)
                .expect("parse");
        assert_eq!(template.normalized(), [-1, 127, -128, 0, 1, -2, 17, -18]);
        assert_eq!(template.encoded_len(), 8);
    }

    #[test]
    fn templates_one_two_and_three_read_only_two_bytes() {
        let data = [0xfe, 0x11];

        for template_id in [
            GenericRegionTemplate::Template1,
            GenericRegionTemplate::Template2,
            GenericRegionTemplate::Template3,
        ] {
            let template =
                GenericRegionAdaptiveTemplate::from(&data, 0, false, template_id).expect("parse");
            assert_eq!(template.normalized(), [-2, 17, -3, -1, 2, -2, 0, 0]);
            assert_eq!(template.encoded_len(), 2);
        }
    }

    #[test]
    fn truncated_template_data_returns_typed_error() {
        let err = GenericRegionAdaptiveTemplate::from(
            &[0x01],
            0,
            false,
            GenericRegionTemplate::Template0,
        )
        .expect_err("expected err");
        assert_eq!(err, Jbig2Error::Truncated("generic adaptive template"));
    }

    #[test]
    fn template_defaults_match_supported_decoder_paths() {
        let template0 =
            GenericRegionAdaptiveTemplate::from(&[], 0, true, GenericRegionTemplate::Template0)
                .expect("parse");
        let template2 =
            GenericRegionAdaptiveTemplate::from(&[], 0, true, GenericRegionTemplate::Template2)
                .expect("parse");

        assert!(template0.is_template0_opt3_default());
        assert!(template2.uses_template23_opt3());
        assert_eq!(template2.normalized(), [2, -1, -3, -1, 2, -2, 0, 0]);
    }

    #[test]
    fn pair_reads_named_coordinate_stride() {
        let template =
            GenericRegionAdaptiveTemplate::from_normalized([3, -1, -3, -1, 2, -2, -2, -2]);

        assert_eq!(template.pair(0), Ok((3, -1)));
        assert_eq!(template.pair(next_gbat_pair_offset(0)), Ok((-3, -1)));
    }
}
