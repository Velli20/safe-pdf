use crate::{compose_op::ComposeOp, error::Jbig2Error, image::JBig2Image, region_info::RegionInfo};
use pdf_utils::BitReader;

use super::{GenericRegionAdaptiveTemplate, GenericRegionFlags, decode_mmr_region};

#[derive(Debug, Clone, Copy)]
pub(crate) struct GenericRegion {
    /// Generic-region bitmap placement and dimensions from section 7.4.1.
    pub(crate) region: RegionInfo,
    /// Generic-region decode flags from section 7.4.6.2.
    pub(crate) flags: GenericRegionFlags,
}

impl TryFrom<&[u8]> for GenericRegion {
    type Error = Jbig2Error;

    /// Parse a generic-region segment header from a byte slice.
    fn try_from(data: &[u8]) -> Result<Self, Self::Error> {
        let mut stream = BitReader::new(data);
        Self::parse(&mut stream)
    }
}

impl GenericRegion {
    /// Parse a JBIG2 generic-region segment header.
    ///
    /// T.88 / ISO/IEC 14492 section 7.4.6.1 defines the generic-region
    /// segment syntax, and section 7.4.6.2 defines the generic-region flags.
    /// When `MMR = 0`, the trailing adaptive-template bytes are parsed and
    /// normalized from the `GBAT` fields into the stored `flags.gbat` table.
    pub(crate) fn parse(stream: &mut BitReader<'_>) -> Result<Self, Jbig2Error> {
        let region = RegionInfo::parse(stream)?;
        let generic_flags = GenericRegionFlags::try_from(stream.try_read_u8::<u8>()?)?;
        let mut flags = generic_flags;
        flags.gbat = GenericRegionAdaptiveTemplate::parse(stream, flags.mmr, flags.gbt_template)?;

        Ok(Self { region, flags })
    }

    /// Decode a JBIG2 generic-region body.
    ///
    /// T.88 / ISO/IEC 14492 section 7.4.6.2 defines the generic-region flags.
    /// The `MMR` flag selects whether the segment body is decoded through the
    /// CCITT path or through the arithmetic generic-region procedure from
    /// section 6.2.5.7. `body` must be the already-sliced segment payload;
    /// width, height, template, and adaptive-template state come from `self`.
    pub(crate) fn decode(&self, body: &[u8]) -> Result<crate::image::JBig2Image, Jbig2Error> {
        if self.flags.mmr {
            decode_mmr_region(self.region.width, self.region.height, body)
        } else {
            self.decode_arithmetic(body)
        }
    }

    /// Decode and compose a JBIG2 generic-region body into `dst`.
    ///
    /// JBIG2 T.88 / ISO/IEC 14492 section 7.4.6 defines the generic-region
    /// segment syntax and flags that drive decoding, while section 6.2.5.7
    /// defines the arithmetic generic-region procedure selected when `MMR = 0`.
    /// The decoded bitmap is placed at the region origin using the compose
    /// operator encoded in the region flags.
    pub(crate) fn compose_to(&self, body: &[u8], dst: &mut JBig2Image) -> Result<(), Jbig2Error> {
        let image = self.decode(body)?;
        image.compose_clipped_to(
            dst,
            i32::from(self.region.x),
            i32::from(self.region.y),
            ComposeOp::from(self.region.flags),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::GenericRegion;
    use crate::{
        arith_decoder::JBig2ArithDecoder,
        compose_op::ComposeOp,
        error::Jbig2Error,
        generic_region::{
            GenericRegionAdaptiveTemplate, GenericRegionFlags, GenericRegionTemplate,
        },
        image::JBig2Image,
        region_info::RegionInfo,
    };
    use pdf_utils::BitReader;

    fn make_region(
        width: u16,
        height: u16,
        mmr: bool,
        template: GenericRegionTemplate,
    ) -> GenericRegion {
        GenericRegion {
            region: RegionInfo {
                width,
                height,
                x: 0,
                y: 0,
                flags: 0,
            },
            flags: GenericRegionFlags {
                mmr,
                gbt_template: template,
                tpgdon: false,
                gbat: GenericRegionAdaptiveTemplate::from(&[], 0, true, template)
                    .expect("template"),
            },
        }
    }

    fn find_arithmetic_body(parsed: &GenericRegion) -> Result<Vec<u8>, Jbig2Error> {
        for byte in 0u8..=u8::MAX {
            let body = [byte];
            if let Ok(image) = parsed.decode(&body)
                && image.width() == parsed.region.width
                && image.height() == parsed.region.height
            {
                return Ok(body.to_vec());
            }
        }

        for hi in 0u8..=u8::MAX {
            for lo in 0u8..=u8::MAX {
                let body = [hi, lo];
                if let Ok(image) = parsed.decode(&body)
                    && image.width() == parsed.region.width
                    && image.height() == parsed.region.height
                {
                    return Ok(body.to_vec());
                }
            }
        }

        Err(Jbig2Error::InvalidState(
            "arithmetic generic region fixture",
        ))
    }

    #[test]
    fn parse_generic_region_leaves_reader_at_body_offset_for_normalized_template() {
        let mut data = Vec::new();
        data.extend_from_slice(&1u32.to_be_bytes());
        data.extend_from_slice(&1u32.to_be_bytes());
        data.extend_from_slice(&0i32.to_be_bytes());
        data.extend_from_slice(&0i32.to_be_bytes());
        data.push(0x00);
        data.push(0x00);
        data.extend_from_slice(&[0x03, 0xff, 0xfd, 0xff, 0x02, 0xfe, 0xfe, 0xfe]);

        let mut stream = BitReader::new(data.as_slice());
        let parsed = GenericRegion::parse(&mut stream).expect("parse");
        let region_bytes = data.get(..17).expect("region bytes");
        assert_eq!(
            parsed.region,
            RegionInfo::try_from(region_bytes).expect("region")
        );
        assert_eq!(stream.byte_pos(), 26);
        assert!(parsed.flags.gbat.is_template0_opt3_default());
        assert_eq!(parsed.flags.gbat.encoded_len(), 8);
        assert_eq!(
            parsed.flags.gbat.normalized(),
            [3, -1, -3, -1, 2, -2, -2, -2]
        );
    }

    #[test]
    fn parse_generic_region_leaves_reader_at_body_offset_for_templates_one_two_and_three() {
        let mut data = Vec::new();
        data.extend_from_slice(&1u32.to_be_bytes());
        data.extend_from_slice(&1u32.to_be_bytes());
        data.extend_from_slice(&0i32.to_be_bytes());
        data.extend_from_slice(&0i32.to_be_bytes());
        data.push(0x00);
        data.push(0x04);
        data.extend_from_slice(&[0x02, 0xff, 0xfd, 0xff, 0x02, 0xfe, 0x00, 0x00]);

        let mut stream = BitReader::new(data.as_slice());
        let parsed = GenericRegion::parse(&mut stream).expect("parse");
        assert_eq!(stream.byte_pos(), 20);
        assert_eq!(parsed.flags.gbt_template, GenericRegionTemplate::Template2);
        assert!(parsed.flags.gbat.uses_template23_opt3());
        assert_eq!(parsed.flags.gbat.encoded_len(), 2);
        assert_eq!(parsed.flags.gbat.normalized(), [2, -1, -3, -1, 2, -2, 0, 0]);
    }

    #[test]
    fn decode_dispatch_uses_parsed_arithmetic_header_fields() {
        let parsed = make_region(1, 1, false, GenericRegionTemplate::Template0);
        let body = find_arithmetic_body(&parsed).expect("arithmetic body");

        let image = parsed.decode(&body).expect("decode");
        assert_eq!(image.width(), 1);
        assert_eq!(image.height(), 1);
    }

    #[test]
    fn decode_dispatch_selects_mmr_path_from_flags() {
        let parsed = make_region(0, 1, true, GenericRegionTemplate::Template0);
        let err = parsed.decode(&[]).expect_err("mmr decode error");
        assert!(matches!(err, Jbig2Error::Ccitt(_)));
    }

    #[test]
    fn synthetic_arithmetic_region_decodes_like_parsed_region() {
        let parsed = make_region(1, 1, false, GenericRegionTemplate::Template0);
        let body = find_arithmetic_body(&parsed).expect("arithmetic body");
        let synthetic = GenericRegion::new_arithmetic(
            parsed.region.width,
            parsed.region.height,
            parsed.flags.gbt_template,
            parsed.flags.tpgdon,
            parsed.flags.gbat,
        )
        .expect("synthetic region");

        let image = synthetic.decode_arithmetic(&body).expect("decode");

        assert_eq!(image.width(), parsed.region.width);
        assert_eq!(image.height(), parsed.region.height);
    }

    #[test]
    fn synthetic_arithmetic_region_advances_shared_decoder() {
        let body = [0x84, 0xc7, 0x73, 0xbf, 0xff, 0xac];
        let synthetic = GenericRegion::new_arithmetic(
            8,
            4,
            GenericRegionTemplate::Template0,
            false,
            GenericRegionAdaptiveTemplate::from(&[], 0, true, GenericRegionTemplate::Template0)
                .expect("template"),
        )
        .expect("synthetic region");
        let mut reader = BitReader::new(&body);
        let image = {
            let mut decoder = JBig2ArithDecoder::new(&mut reader);
            synthetic
                .decode_arithmetic_with_decoder(&mut decoder, None)
                .expect("decode")
        };

        assert_eq!(image.width(), 8);
        assert_eq!(image.height(), 4);
        assert!(reader.byte_pos() > 0);
    }

    #[test]
    fn compose_to_matches_decode_then_compose() {
        let parsed = make_region(1, 1, false, GenericRegionTemplate::Template2);
        let body = find_arithmetic_body(&parsed).expect("arithmetic body");
        let mut expected = JBig2Image::new(3, 3);
        parsed.decode(&body).expect("decode").compose_clipped_to(
            &mut expected,
            i32::from(parsed.region.x),
            i32::from(parsed.region.y),
            ComposeOp::from(parsed.region.flags),
        );

        let mut actual = JBig2Image::new(3, 3);
        parsed.compose_to(&body, &mut actual).expect("compose");

        assert_eq!(actual.to_tight_bytes(), expected.to_tight_bytes());
    }
}
