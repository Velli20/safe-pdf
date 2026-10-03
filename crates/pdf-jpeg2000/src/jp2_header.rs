//! Image, palette, and channel boxes inside a JP2 or JPX header superbox.
//!
//! A JP2 Header box describes the whole file. A JPX Codestream Header box
//! describes one codestream and overrides those defaults box by box, so the
//! same collector reads both and the validation runs once on the merged result.

use pdf_graphics::Size;
use pdf_utils::BitReader;

use crate::{
    Jpeg2000Error,
    box_reader::{BoxKind, Jp2Box},
    channel_definition::ChannelDefinitions,
    color_specifications::{ColorDescription, ColorSpecifications},
    component_map::ComponentMapping,
    container::{InputFormat, Jp2Metadata},
    jp2::{ContainerError, Resolution},
    palette::Palette,
    size::ImageShape,
};

/// Byte count of an Image Header box body.
const IMAGE_HEADER_BYTES: usize = 14;
/// Part 1 JPEG 2000 compression type in an Image Header box.
const JPEG2000_COMPRESSION: u8 = 7;
/// Value indicating per-component bit depths in a separate bpcc box.
const VARIABLE_BIT_DEPTH: u8 = 0xff;

/// The Image Header box fields this decoder interprets.
struct ImageHeaderBox {
    shape: ImageShape,
    variable_depth: bool,
}

impl TryFrom<Jp2Box<'_>> for ImageHeaderBox {
    type Error = Jpeg2000Error;

    fn try_from(box_view: Jp2Box<'_>) -> Result<Self, Self::Error> {
        if box_view.payload().len() != IMAGE_HEADER_BYTES {
            return Err(ContainerError::from(box_view).into());
        }
        let mut data = BitReader::new(box_view.payload());
        let height = data.try_read_u32_be::<u32>()?;
        let width = data.try_read_u32_be::<u32>()?;
        let components = data.try_read_u16_be::<u16>()?;
        let depth = data.try_read_u8::<u8>()?;
        let compression = data.try_read_u8::<u8>()?;
        let unknown_color = data.try_read_u8::<u8>()?;
        let intellectual_property = data.try_read_u8::<u8>()?;
        if width == 0
            || height == 0
            || components == 0
            || compression != JPEG2000_COMPRESSION
            || unknown_color > 1
            || intellectual_property > 1
        {
            return Err(ContainerError::from(box_view).into());
        }
        Ok(Self {
            shape: ImageShape {
                size: Size { width, height },
                components,
            },
            variable_depth: depth == VARIABLE_BIT_DEPTH,
        })
    }
}

/// Header child boxes collected during one scan of a header superbox.
///
/// Colour specifications are not collected here: they are read through
/// [`ColorSpecifications`] from whichever superbox carries them, because a JPX
/// compositing layer may supply its own colour group.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct HeaderBoxes<'a> {
    image: Option<Jp2Box<'a>>,
    bits_per_component: Option<Jp2Box<'a>>,
    palette: Option<Jp2Box<'a>>,
    component_mapping: Option<Jp2Box<'a>>,
    channel_definitions: Option<Jp2Box<'a>>,
}

impl<'a> HeaderBoxes<'a> {
    /// Collects the recognised children of a header superbox.
    ///
    /// Annex I requires the Image Header box to open a JP2 Header box, while a
    /// JPX Codestream Header box overrides only the boxes it carries, so
    /// `require_image` distinguishes the two.
    ///
    /// # Errors
    ///
    /// Returns a container error for a repeated singleton box or an absent
    /// required Image Header box, and a box error for unframable children.
    pub(crate) fn collect(
        box_view: Jp2Box<'a>,
        require_image: bool,
    ) -> Result<Self, Jpeg2000Error> {
        let mut children = box_view.children();
        let mut boxes = Self::default();
        if require_image {
            boxes.image = Some(children.require(BoxKind::ImageHeader)?);
        }
        while let Some(child) = children.next_box()? {
            boxes.insert(child)?;
        }
        Ok(boxes)
    }

    /// Records a recognised child box, rejecting a repeated singleton box.
    ///
    /// Unrecognised boxes are skipped, as Annex I allows.
    fn insert(&mut self, child: Jp2Box<'a>) -> Result<(), ContainerError> {
        let kind = child.kind();
        let slot = match kind {
            BoxKind::ImageHeader => &mut self.image,
            BoxKind::BitsPerComponent => &mut self.bits_per_component,
            BoxKind::Palette => &mut self.palette,
            BoxKind::ComponentMapping => &mut self.component_mapping,
            BoxKind::ChannelDefinition => &mut self.channel_definitions,
            _ => return Ok(()),
        };
        if slot.is_some() {
            return Err(ContainerError::Duplicate {
                offset: child.offset(),
                kind,
            });
        }
        *slot = Some(child);
        Ok(())
    }

    /// Lays one codestream's own header boxes over these defaults.
    pub(crate) fn overridden_by(self, overrides: Self) -> Self {
        Self {
            image: overrides.image.or(self.image),
            bits_per_component: overrides.bits_per_component.or(self.bits_per_component),
            palette: overrides.palette.or(self.palette),
            component_mapping: overrides.component_mapping.or(self.component_mapping),
            channel_definitions: overrides.channel_definitions.or(self.channel_definitions),
        }
    }

    /// Validates the collected boxes against the Image Header box.
    ///
    /// # Errors
    ///
    /// Returns a container error when a required box is absent, when a box
    /// contradicts the Image Header box, or when the channel chain names a
    /// component, palette column, or channel that does not exist.
    pub(crate) fn into_header(
        self,
        colors: Option<ColorSpecifications<'a>>,
        format: InputFormat,
    ) -> Result<Jp2Header<'a>, Jpeg2000Error> {
        let image = ImageHeaderBox::try_from(
            self.image
                .ok_or(ContainerError::Required(BoxKind::ImageHeader))?,
        )?;
        let components = image.shape.components;
        if image.variable_depth != self.bits_per_component.is_some() {
            return Err(ContainerError::Required(BoxKind::BitsPerComponent).into());
        }
        if let Some(bits) = self
            .bits_per_component
            .filter(|bits| bits.payload().len() != usize::from(components))
        {
            return Err(ContainerError::from(bits).into());
        }
        let color = match colors {
            Some(list) => list.primary()?,
            None => None,
        };
        if color.is_none() && format == InputFormat::Jp2 {
            return Err(ContainerError::Required(BoxKind::ColorSpecification).into());
        }
        let palette = self.palette.map(Palette::try_from).transpose()?;
        let component_mapping = match self.component_mapping {
            Some(box_view) => {
                let mapping = ComponentMapping::try_from(box_view)?;
                mapping.validate(box_view, components, palette.as_ref())?;
                Some(mapping)
            }
            // A palette is meaningless without a mapping that reads it, so
            // Annex I requires the two boxes together.
            None if palette.is_some() => {
                return Err(ContainerError::Required(BoxKind::ComponentMapping).into());
            }
            None => None,
        };
        let channels = component_mapping.map_or(components, ComponentMapping::channel_count);
        let channel_definitions = match self.channel_definitions {
            Some(box_view) => {
                let definitions = ChannelDefinitions::try_from(box_view)?;
                definitions.validate(box_view, channels)?;
                Some(definitions)
            }
            None => None,
        };
        Ok(Jp2Header {
            shape: image.shape,
            color,
            colors,
            palette,
            component_mapping,
            channel_definitions,
        })
    }
}

/// Validated header contents, before a codestream is associated with them.
pub(crate) struct Jp2Header<'a> {
    shape: ImageShape,
    color: Option<ColorDescription<'a>>,
    colors: Option<ColorSpecifications<'a>>,
    palette: Option<Palette<'a>>,
    component_mapping: Option<ComponentMapping<'a>>,
    channel_definitions: Option<ChannelDefinitions<'a>>,
}

impl<'a> Jp2Header<'a> {
    /// Returns the image shape the header declares.
    pub(crate) fn shape(&self) -> ImageShape {
        self.shape
    }

    /// Attaches the resolved codestream and layer without copying either.
    pub(crate) fn into_metadata(
        self,
        codestream: &'a [u8],
        resolution: Resolution<'a>,
    ) -> Jp2Metadata<'a> {
        Jp2Metadata {
            codestream,
            shape: self.shape,
            color: self.color,
            colors: self.colors,
            palette: self.palette,
            component_mapping: self.component_mapping,
            channel_definitions: self.channel_definitions,
            association: resolution.association,
            opacity: resolution.opacity,
            requirements: resolution.requirements,
            composition: resolution.composition,
        }
    }
}
