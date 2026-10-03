//! The JPX Compositing Layer Header superbox.
//!
//! Part 2 Annex M describes one layer of a composed image: which codestreams
//! supply it, how it expresses opacity, and which colour descriptions apply to
//! it. A layer's own boxes take precedence over the JP2 Header defaults, and a
//! layer without a Codestream Registration box uses the codestream that shares
//! its index.

use crate::{
    Jpeg2000Error,
    box_reader::{BoxKind, Jp2Box},
    codestream_registration::CodestreamRegistration,
    color_specifications::ColorSpecifications,
    container::InputFormat,
    jp2::ContainerError,
    opacity::Opacity,
};

/// One compositing layer's own header boxes, borrowed from the file.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CompositingLayer<'a> {
    colors: Option<ColorSpecifications<'a>>,
    opacity: Option<Opacity<'a>>,
    registration: Option<CodestreamRegistration<'a>>,
}

impl<'a> CompositingLayer<'a> {
    /// Reads the colour group, opacity, and registration of one layer.
    ///
    /// # Errors
    ///
    /// Returns a container error when a child box is repeated or malformed,
    /// and a box error when a child cannot be framed.
    pub(crate) fn parse(box_view: Jp2Box<'a>, format: InputFormat) -> Result<Self, Jpeg2000Error> {
        let mut layer = Self::default();
        let mut children = box_view.children();
        while let Some(child) = children.next_box()? {
            let kind = child.kind();
            let duplicate = || ContainerError::Duplicate {
                offset: child.offset(),
                kind,
            };
            match kind {
                BoxKind::ColorGroup => {
                    if layer.colors.is_some() {
                        return Err(duplicate().into());
                    }
                    layer.colors = Some(ColorSpecifications::new(child, format));
                }
                BoxKind::Opacity => {
                    if layer.opacity.is_some() {
                        return Err(duplicate().into());
                    }
                    layer.opacity = Some(Opacity::try_from(child)?);
                }
                BoxKind::CodestreamRegistration => {
                    if layer.registration.is_some() {
                        return Err(duplicate().into());
                    }
                    layer.registration = Some(CodestreamRegistration::try_from(child)?);
                }
                _ => {}
            }
        }
        Ok(layer)
    }

    /// Returns the colour descriptions this layer carries, when it has any.
    pub fn colors(&self) -> Option<ColorSpecifications<'a>> {
        self.colors
    }

    /// Returns how this layer expresses opacity, when it says.
    pub fn opacity(&self) -> Option<Opacity<'a>> {
        self.opacity
    }

    /// Returns the layer's codestream registration, when it has one.
    pub fn registration(&self) -> Option<CodestreamRegistration<'a>> {
        self.registration
    }
}
