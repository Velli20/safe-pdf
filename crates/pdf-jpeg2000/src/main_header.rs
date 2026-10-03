//! Main-header sequencing up to the first SOT marker.

use crate::{
    Jpeg2000Error,
    codestream::{MainHeader, Marker, MarkerReader, MarkerSegment, SizeHeader},
    coding::{
        CodingParameters, CodingStyle, ComponentCodingStyle, ProgressionChanges, RegionOfInterest,
    },
    offset_site::OffsetSite,
    quantization::{ComponentQuantization, Quantization},
};

/// Main-header segments collected before the first SOT marker.
///
/// Per-component overrides are validated once the COD and QCD defaults are
/// known, so only the defaults and the PPM flag are recorded here.
#[derive(Default)]
struct HeaderMarkers<'a> {
    coding: Option<CodingStyle<'a>>,
    quantization: Option<Quantization<'a>>,
}

impl<'a> HeaderMarkers<'a> {
    /// Records one main-header segment, rejecting a misplaced marker.
    fn insert(&mut self, segment: MarkerSegment<'a>) -> Result<(), Jpeg2000Error> {
        match segment.marker() {
            Marker::Cod if self.coding.is_none() => {
                self.coding = Some(CodingStyle::try_from(segment)?);
            }
            Marker::Qcd if self.quantization.is_none() => {
                self.quantization = Some(Quantization::parse(segment)?);
            }
            // Packed packet headers, component overrides, and informational
            // segments are read where they are needed: PPM by the tile packet
            // reader, and the overrides by `MainHeader::validate_overrides`
            // once the defaults are known.
            Marker::Ppm
            | Marker::Coc
            | Marker::Qcc
            | Marker::Rgn
            | Marker::Poc
            | Marker::Tlm
            | Marker::Plm
            | Marker::Crg
            | Marker::Com => {}
            Marker::Cap | Marker::Prf | Marker::Unknown(_) => {
                return Err(Jpeg2000Error::UnsupportedFeature {
                    feature: "codestream extension marker",
                });
            }
            _ => {
                return segment.site().out_of_order();
            }
        }
        Ok(())
    }

    /// Builds the validated header once the first SOT marker is reached.
    ///
    /// `header_len` is the byte length of the markers before that SOT, which
    /// splits the codestream into the header and the tile-part data.
    fn into_header(
        self,
        bytes: &'a [u8],
        offset: usize,
        header_len: usize,
        size: SizeHeader<'a>,
        sot: MarkerSegment<'a>,
    ) -> Result<MainHeader<'a>, Jpeg2000Error> {
        let (Some(coding), Some(quantization)) = (self.coding, self.quantization) else {
            return sot.site().out_of_order();
        };
        let (raw, body) = bytes
            .split_at_checked(header_len)
            .ok_or(Jpeg2000Error::Overflow {
                context: "main-header length",
            })?;
        let header = MainHeader {
            raw,
            body,
            offset,
            body_site: OffsetSite::new(offset.saturating_add(header_len)),
            size,
            coding,
            quantization,
        };
        header.validate_overrides()?;
        Ok(header)
    }
}

impl<'a> MainHeader<'a> {
    /// Parses the borrowed main header without entering tile-part data.
    ///
    /// `offset` is the codestream's position in the caller's input, so reported
    /// offsets stay meaningful for a codestream inside a JP2 container.
    pub(crate) fn parse(bytes: &'a [u8], offset: usize) -> Result<Self, Jpeg2000Error> {
        let mut reader = MarkerReader::new_at(bytes, offset);
        reader.require(Marker::Soc)?;
        let size = SizeHeader::try_from(reader.require(Marker::Siz)?)?;
        let mut markers = HeaderMarkers::default();
        loop {
            let header_len = reader.consumed();
            let segment = reader.next_required()?;
            if segment.marker() == Marker::Sot {
                return markers.into_header(bytes, offset, header_len, size, segment);
            }
            markers.insert(segment)?;
        }
    }

    /// Returns the coding parameters that apply to one component.
    ///
    /// A COC segment for the component replaces the COD defaults; otherwise the
    /// defaults apply. The overrides are re-read from the borrowed header rather
    /// than copied while parsing.
    pub(crate) fn component_parameters(
        &self,
        component: u16,
    ) -> Result<CodingParameters<'a>, Jpeg2000Error> {
        let components = self.size.component_count();
        let parameters = ComponentCodingStyle::find(self.markers(), component, components)?;
        Ok(parameters.unwrap_or(self.coding.parameters))
    }

    /// Cross-checks the per-component overrides against the COD defaults.
    fn validate_overrides(&self) -> Result<(), Jpeg2000Error> {
        let components = self.size.component_count();
        self.quantization.validate_subbands(self.parameters())?;
        let mut markers = self.markers();
        while let Some(segment) = markers.next_segment()? {
            self.validate_override(segment, components)?;
        }
        Ok(())
    }

    /// Validates one main-header override segment.
    fn validate_override(
        &self,
        segment: MarkerSegment<'a>,
        components: u16,
    ) -> Result<(), Jpeg2000Error> {
        match segment.marker() {
            Marker::Coc => {
                ComponentCodingStyle::parse(segment, components)?;
            }
            Marker::Qcc => {
                let override_ = ComponentQuantization::parse(segment, components)?;
                let parameters = self.component_parameters(override_.component)?;
                override_.quantization.validate_subbands(&parameters)?;
            }
            Marker::Rgn => {
                RegionOfInterest::parse(segment, components)?;
            }
            Marker::Poc => {
                ProgressionChanges::parse(segment, components, &self.coding)?;
            }
            _ => {}
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use pdf_graphics::Size;

    use crate::{Decoder, DecoderLimits, DecoderOptions, InputFormat};

    #[test]
    fn reads_cached_part_one_headers() {
        let limits = DecoderLimits {
            max_input_bytes: 2_000_000,
            max_pixels: 2_000_000,
            max_components: 8,
            max_tiles: 1_024,
            max_working_bytes: 2_000_000,
        };
        let cases = [
            ("p0_01.j2k", 128, 128, 1),
            ("p0_03.j2k", 256, 256, 1),
            ("p0_04.j2k", 640, 480, 3),
            ("p0_05.j2k", 1_024, 1_024, 4),
            ("p0_10.j2k", 256, 256, 3),
        ];
        for (name, width, height, components) in cases {
            let path = format!(
                "{}/fixtures/cache/openjpeg-data/input/conformance/{name}",
                env!("CARGO_MANIFEST_DIR")
            );
            let Ok(bytes) = fs::read(path) else { continue };
            let options = DecoderOptions {
                format: InputFormat::Codestream,
                limits,
            };
            let decoder = Decoder::new(&bytes, options).parse_header().expect(name);
            let size = decoder.header().main().size();
            assert_eq!(
                (size.image_size(), size.component_count()),
                (Size { width, height }, components),
                "{name}"
            );
        }
    }
}
