//! Borrowed views of the JP2 and JPX file formats.
//!
//! JP2 boxes surround a contiguous JPEG 2000 codestream and may describe
//! colour, palette, component mapping, and channel meaning; JPX adds several
//! codestreams, per-codestream and per-layer header boxes, opacity, and a
//! composition of layers. Payloads remain in the caller's byte slice and are
//! interpreted, not converted: the palette is read one entry at a time rather
//! than expanded, and no ICC colour transform is applied. [`ImageChannels`]
//! resolves the boxes into the output channels a renderer consumes.

use pdf_graphics::Size;

use crate::size::ImageShape;

pub use crate::channel_definition::{
    ChannelAssociation, ChannelDefinition, ChannelDefinitions, ChannelKind,
};
pub use crate::codestream_registration::{CodestreamRegistration, CodestreamRegistrationRecord};
pub use crate::color_space::EnumeratedColorSpace;
pub use crate::color_specifications::{ColorDescription, ColorSpecifications};
pub use crate::component_map::{ChannelSource, ComponentMapping};
pub use crate::compositing_layer::CompositingLayer;
pub use crate::composition::Composition;
pub use crate::fragment_list::{Fragment, FragmentList};
pub use crate::image_channels::{ImageChannels, OutputChannel};
pub use crate::opacity::{Opacity, OpacityKind};
pub use crate::palette::{Palette, PaletteColumn};
pub use crate::reader_requirements::ReaderRequirements;

/// Selects the byte format supplied to [`crate::Decoder`].
///
/// `Auto` distinguishes a raw SOC marker from a JP2-family signature box.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum InputFormat {
    /// Detect a raw Part 1 codestream or Part 1 JP2 container.
    Auto,
    /// Require bytes beginning with the raw codestream SOC marker.
    Codestream,
    /// Require a JP2 box container with a contiguous codestream box.
    Jp2,
    /// Require a JPX container using Part 1 codestreams.
    Jpx,
}

pub use crate::box_reader::{BoxKind, Jp2Box, Jp2BoxReader};

/// Colour interpretation supplied by a Colour Specification box.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ColorSpecification<'a> {
    /// An enumerated JP2 colour-space code, such as sRGB or greyscale.
    Enumerated {
        /// Numeric enumeration from the JP2 box.
        code: u32,
    },
    /// An ICC profile borrowed from the JP2 box body.
    Icc {
        /// ICC profile bytes borrowed from the source slice.
        profile: &'a [u8],
    },
}

/// How a container's codestreams and compositing layers were resolved.
///
/// A JP2 file has exactly one of each. A JPX file may carry several, and this
/// decoder reconstructs the codestream belonging to the first compositing
/// layer, so the counts record what the file holds and the indices record what
/// was decoded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CodestreamAssociation {
    /// Number of codestreams the file carries, counting fragment tables.
    pub codestream_count: u32,
    /// Number of compositing layers the file declares.
    pub layer_count: u32,
    /// Zero-based index of the codestream this decode reconstructs.
    pub codestream: u32,
    /// Zero-based index of the compositing layer that codestream supplies.
    pub layer: u32,
}

/// Parsed container metadata alongside its borrowed codestream.
#[derive(Clone, Debug, PartialEq)]
pub struct Jp2Metadata<'a> {
    pub(crate) codestream: &'a [u8],
    pub(crate) shape: ImageShape,
    pub(crate) color: Option<ColorDescription<'a>>,
    pub(crate) colors: Option<ColorSpecifications<'a>>,
    pub(crate) palette: Option<Palette<'a>>,
    pub(crate) component_mapping: Option<ComponentMapping<'a>>,
    pub(crate) channel_definitions: Option<ChannelDefinitions<'a>>,
    pub(crate) association: CodestreamAssociation,
    pub(crate) opacity: Option<Opacity<'a>>,
    pub(crate) requirements: Option<ReaderRequirements<'a>>,
    pub(crate) composition: Option<Composition>,
}

impl<'a> Jp2Metadata<'a> {
    /// Returns the image shape declared by the Image Header box.
    pub(crate) fn shape(&self) -> ImageShape {
        self.shape
    }

    /// Borrows the bytes of the codestream this decode reconstructs.
    pub fn codestream(&self) -> &'a [u8] {
        self.codestream
    }

    /// Returns the image extent declared by the Image Header box.
    pub fn image_size(&self) -> Size<u32> {
        self.shape.size
    }

    /// Returns the component count declared by the Image Header box.
    pub fn component_count(&self) -> u16 {
        self.shape.components
    }

    /// Returns how the file's codestreams and layers were resolved.
    pub fn association(&self) -> CodestreamAssociation {
        self.association
    }

    /// Returns the colour description this decoder uses, without converting it.
    pub fn color_specification(&self) -> Option<ColorSpecification<'a>> {
        Some(self.color?.specification())
    }

    /// Returns that description with the fields that rank it among alternatives.
    pub fn color_description(&self) -> Option<ColorDescription<'a>> {
        self.color
    }

    /// Returns every colour description the resolved layer offers.
    ///
    /// Part 2 lets a file describe its colour several ways, each with a
    /// precedence and an approximation. A renderer that can honour a
    /// description this decoder does not prefer may choose from this list
    /// instead of [`Self::color_specification`].
    pub fn color_specifications(&self) -> Option<ColorSpecifications<'a>> {
        self.colors
    }

    /// Returns the validated palette when the header contains one.
    pub fn palette(&self) -> Option<Palette<'a>> {
        self.palette
    }

    /// Returns the component-to-channel mapping when the header defines one.
    pub fn component_mapping(&self) -> Option<ComponentMapping<'a>> {
        self.component_mapping
    }

    /// Returns the colour and opacity channel definitions, when present.
    pub fn channel_definitions(&self) -> Option<ChannelDefinitions<'a>> {
        self.channel_definitions
    }

    /// Returns the resolved layer's Opacity box, when it has one.
    ///
    /// A JPX compositing layer may declare opacity here instead of in a
    /// Channel Definition box.
    pub fn opacity(&self) -> Option<Opacity<'a>> {
        self.opacity
    }

    /// Returns the JPX Reader Requirements box, when the file carries one.
    ///
    /// The box lists the features a reader needs. Its standard flag numbering
    /// is not modelled here, so it is reported rather than enforced: a feature
    /// this decoder cannot honour is refused by the box or marker that carries
    /// it, not by this list.
    pub fn requirements(&self) -> Option<ReaderRequirements<'a>> {
        self.requirements
    }

    /// Returns the composition the file declares, when it declares one.
    ///
    /// A composition this decoder accepts leaves the resolved layer alone, so
    /// its canvas is the image extent; the box is reported because it says the
    /// file was written as a composed image.
    pub fn composition(&self) -> Option<Composition> {
        self.composition
    }
}
