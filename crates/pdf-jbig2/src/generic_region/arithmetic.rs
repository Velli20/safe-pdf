use crate::{
    arith_decoder::JBig2ArithDecoder, error::Jbig2Error, image::JBig2Image, region_info::RegionInfo,
};
use pdf_utils::BitReader;

use super::{
    GenericRegion, GenericRegionAdaptiveTemplate, GenericRegionFlags, GenericRegionTemplate,
    tables::Opt3TemplateConfig,
};

impl GenericRegion {
    /// Construct a normalized arithmetic generic-region description.
    ///
    /// T.88 / ISO/IEC 14492 section 7.4.6 defines the generic-region segment
    /// header fields that feed arithmetic decoding. Some higher-level JBIG2
    /// procedures, including pattern dictionaries, symbol dictionaries, and
    /// halftone gray-plane decoding, reuse that arithmetic procedure without
    /// carrying a wire-level generic-region segment. This constructor packages
    /// the normalized width, height, template, `TPGDON`, and adaptive-template
    /// (`GBAT`) inputs into the same internal representation used by parsed
    /// generic-region segments.
    pub(crate) fn new_arithmetic(
        width: u16,
        height: u16,
        gbt_template: GenericRegionTemplate,
        tpgdon: bool,
        gbat: GenericRegionAdaptiveTemplate,
    ) -> Result<Self, Jbig2Error> {
        Ok(Self {
            region: RegionInfo {
                width,
                height,
                x: 0,
                y: 0,
                flags: 0,
            },
            flags: GenericRegionFlags {
                mmr: false,
                gbt_template,
                tpgdon,
                gbat,
            },
        })
    }

    /// Decode arithmetic-coded generic-region data from a byte slice.
    ///
    /// T.88 / ISO/IEC 14492 section 6.2.5.7 defines the arithmetic generic-
    /// region decoding procedure, while section 7.4.6 supplies the region
    /// dimensions, template id, `TPGDON`, and `GBAT` adaptive-template offsets.
    /// This method consumes a byte-aligned generic-region body and creates the
    /// arithmetic decoder internally from the provided payload.
    pub(crate) fn decode_arithmetic(&self, data: &[u8]) -> Result<JBig2Image, Jbig2Error> {
        let mut stream = BitReader::new(data);
        let mut decoder = JBig2ArithDecoder::new(&mut stream);
        self.decode_arithmetic_with_decoder(&mut decoder, None)
    }

    /// Decode arithmetic-coded generic-region data with a shared decoder.
    ///
    /// T.88 / ISO/IEC 14492 section 6.2.5.7 defines the arithmetic generic-
    /// region procedure that this method executes. Section 7.4.6 defines the
    /// template and adaptive-template parameters stored on `self`. `skip`
    /// supplies the optional skip bitmap used by higher-level procedures such
    /// as halftone gray-image decoding in section 6.6.5.
    pub(crate) fn decode_arithmetic_with_decoder(
        &self,
        decoder: &mut JBig2ArithDecoder<'_, '_>,
        skip: Option<&JBig2Image>,
    ) -> Result<JBig2Image, Jbig2Error> {
        let width = self.region.width;
        let height = self.region.height;
        let flags = self.flags;

        if !JBig2Image::is_valid_image_size(width, height) {
            return JBig2Image::try_new(width, height, None);
        }

        match flags.gbt_template {
            GenericRegionTemplate::Template0
                if skip.is_none() && flags.gbat.is_template0_opt3_default() =>
            {
                decoder.decode_arith_opt3(
                    width,
                    height,
                    flags.tpgdon,
                    Opt3TemplateConfig::TEMPLATE0,
                )
            }
            GenericRegionTemplate::Template2
                if skip.is_none() && flags.gbat.uses_template23_opt3() =>
            {
                decoder.decode_arith_opt3(
                    width,
                    height,
                    flags.tpgdon,
                    Opt3TemplateConfig::TEMPLATE2,
                )
            }
            template => decoder.decode_arith_template_unopt(
                width,
                height,
                template,
                flags.tpgdon,
                &flags.gbat,
                skip,
            ),
        }
    }
}
