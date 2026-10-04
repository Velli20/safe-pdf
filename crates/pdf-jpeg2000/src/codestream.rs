//! Borrowed image and tile-part metadata from the codestream.

use crate::{
    coding::{CodingParameters, CodingStyle},
    container::{
        ColorDescription, ColorSpecification, ColorSpecifications, ImageChannels, InputFormat,
        Jp2Metadata,
    },
    offset_site::OffsetSite,
    quantization::Quantization,
};

pub use crate::marker_reader::{Marker, MarkerReader, MarkerSegment};
pub use crate::size::{ComponentInfo, SizeHeader};
pub use crate::tile_part::{TilePart, TilePartReader};

/// Validated main header of a Part 1 codestream.
///
/// SIZ supplies image geometry while COD and QCD provide the default coding and
/// quantization parameters. Marker bodies borrow the input, and per-component
/// overrides are re-read from [`MainHeader::markers`] rather than copied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MainHeader<'a> {
    pub(crate) raw: &'a [u8],
    pub(crate) body: &'a [u8],
    pub(crate) offset: usize,
    pub(crate) body_site: OffsetSite,
    pub(crate) size: SizeHeader<'a>,
    pub(crate) coding: CodingStyle<'a>,
    pub(crate) quantization: Quantization<'a>,
}

impl<'a> MainHeader<'a> {
    /// Returns the parsed SIZ geometry and borrowed component table.
    pub fn size(&self) -> &SizeHeader<'a> {
        &self.size
    }

    /// Creates a borrowing reader over the original main-header markers.
    pub fn markers(&self) -> MarkerReader<'a> {
        MarkerReader::new_at(self.raw, self.offset)
    }

    /// Returns the COD coding defaults for every tile and component.
    pub(crate) fn coding(&self) -> &CodingStyle<'a> {
        &self.coding
    }

    /// Returns the default coding parameters of the codestream.
    pub(crate) fn parameters(&self) -> &CodingParameters<'a> {
        &self.coding.parameters
    }

    /// Borrows the codestream bytes that follow the main header.
    pub(crate) fn body(&self) -> &'a [u8] {
        self.body
    }

    /// Returns the absolute byte offset of the first tile-part.
    pub(crate) fn body_offset(&self) -> usize {
        self.body_site.offset()
    }

    /// Returns the first tile-part position for structural diagnostics.
    pub(crate) fn body_site(&self) -> OffsetSite {
        self.body_site
    }
}

/// Unified image header for a raw codestream or a JP2-contained codestream.
///
/// JP2 metadata is present only when a box container was supplied.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageHeader<'a> {
    pub(crate) main: MainHeader<'a>,
    pub(crate) format: InputFormat,
    pub(crate) jp2: Option<Jp2Metadata<'a>>,
}

impl<'a> ImageHeader<'a> {
    /// Returns the detected input format after header validation.
    pub fn format(&self) -> InputFormat {
        self.format
    }

    /// Returns the main codestream header shared by both input forms.
    pub fn main(&self) -> &MainHeader<'a> {
        &self.main
    }

    /// Returns JP2 colour and channel metadata when a container was present.
    pub fn jp2(&self) -> Option<&Jp2Metadata<'a>> {
        self.jp2.as_ref()
    }

    /// Returns the JP2 colour description, when the container supplies one.
    ///
    /// A PDF `/ColorSpace` entry takes precedence over this description for a
    /// JPX image XObject, so a renderer consults it only in that entry's
    /// absence.
    pub fn color_specification(&self) -> Option<ColorSpecification<'a>> {
        self.jp2.as_ref()?.color_specification()
    }

    /// Returns that description with its precedence and approximation.
    pub fn color_description(&self) -> Option<ColorDescription<'a>> {
        self.jp2.as_ref()?.color_description()
    }

    /// Returns every colour description the resolved layer offers.
    ///
    /// A renderer able to honour a description this crate does not prefer may
    /// choose from the list instead of [`Self::color_specification`].
    pub fn color_specifications(&self) -> Option<ColorSpecifications<'a>> {
        self.jp2.as_ref()?.color_specifications()
    }

    /// Resolves the output channels this image presents to a renderer.
    ///
    /// Without a container each SIZ component is its own colour channel. With
    /// one, the Palette, Component Mapping, and Channel Definition boxes
    /// decide the count, the order, and which channel carries opacity, and a
    /// JPX compositing layer's Opacity box can name that channel instead.
    pub fn channels(&self) -> ImageChannels<'a> {
        match self.jp2.as_ref() {
            Some(jp2) => ImageChannels::new(
                self.main.size,
                jp2.palette(),
                jp2.component_mapping(),
                jp2.channel_definitions(),
                jp2.opacity().and_then(|opacity| opacity.channel_kind()),
            ),
            None => ImageChannels::new(self.main.size, None, None, None, None),
        }
    }
}
