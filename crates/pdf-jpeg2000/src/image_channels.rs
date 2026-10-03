//! The output channels a JPEG 2000 image presents to a renderer.
//!
//! A raw codestream has one output channel per SIZ component. A JP2 or JPX
//! container may reorder them, expand one component through a palette, and
//! label each result as colour or opacity. This module resolves those boxes
//! against SIZ so a PDF image path can ask what each channel carries, how wide
//! it is, and which codestream component supplies it, without repeating the
//! Annex I rules.

use crate::{
    Jpeg2000Error,
    box_reader::BoxKind,
    channel_definition::{ChannelAssociation, ChannelDefinition, ChannelDefinitions, ChannelKind},
    component_map::{ChannelSource, ComponentMapping},
    jp2::ContainerError,
    palette::Palette,
    size::SizeHeader,
};

/// One resolved output channel of a decoded image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputChannel {
    /// Zero-based index of this channel in the image's channel order.
    pub index: u16,
    /// The codestream component and optional palette column behind it.
    pub source: ChannelSource,
    /// Sample precision in bits after any palette lookup.
    pub precision: u8,
    /// Whether the channel's samples are signed after any palette lookup.
    pub signed: bool,
    /// What the channel carries.
    pub kind: ChannelKind,
    /// What the channel applies to.
    pub association: ChannelAssociation,
}

impl OutputChannel {
    /// Returns whether the channel carries opacity in either form.
    pub fn is_opacity(self) -> bool {
        matches!(
            self.kind,
            ChannelKind::Opacity | ChannelKind::PremultipliedOpacity
        )
    }
}

/// The resolved channel list of one image, borrowed from its headers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageChannels<'a> {
    size: SizeHeader<'a>,
    palette: Option<Palette<'a>>,
    mapping: Option<ComponentMapping<'a>>,
    definitions: Option<ChannelDefinitions<'a>>,
    opacity: Option<ChannelKind>,
}

impl<'a> ImageChannels<'a> {
    /// Combines SIZ with the optional JP2 channel chain.
    ///
    /// `opacity` is what a JPX compositing layer's Opacity box says its last
    /// channel carries, which describes a channel no Channel Definition record
    /// covers.
    pub(crate) fn new(
        size: SizeHeader<'a>,
        palette: Option<Palette<'a>>,
        mapping: Option<ComponentMapping<'a>>,
        definitions: Option<ChannelDefinitions<'a>>,
        opacity: Option<ChannelKind>,
    ) -> Self {
        Self {
            size,
            palette,
            mapping,
            definitions,
            opacity,
        }
    }

    /// Returns the number of output channels the image presents.
    ///
    /// A Component Mapping box sets the count; without one it is the SIZ
    /// component count.
    pub fn count(&self) -> u16 {
        self.mapping.map_or_else(
            || self.size.component_count(),
            ComponentMapping::channel_count,
        )
    }

    /// Returns whether the image presents no output channel.
    pub fn is_empty(&self) -> bool {
        self.count() == 0
    }

    /// Resolves one output channel.
    ///
    /// # Errors
    ///
    /// Returns `InvalidBox`-derived container errors when the channel chain
    /// names a component or palette column that does not exist, and
    /// `LimitExceeded` or SIZ errors when the component index is out of range.
    pub fn channel(&self, index: u16) -> Result<OutputChannel, Jpeg2000Error> {
        if index >= self.count() {
            return Err(ContainerError::Channel { channel: index }.into());
        }
        let source = match self.mapping {
            Some(mapping) => mapping
                .channel(index)
                .ok_or(ContainerError::Channel { channel: index })?,
            None => ChannelSource::Direct { component: index },
        };
        let (precision, signed) = self.sample_layout(source)?;
        let definition = self.definitions.and_then(|records| records.get(index));
        let (kind, association) = definition.map_or_else(
            || self.undescribed_channel(index),
            |ChannelDefinition {
                 kind, association, ..
             }| (kind, association),
        );
        Ok(OutputChannel {
            index,
            source,
            precision,
            signed,
            kind,
            association,
        })
    }

    /// Returns every output channel in order.
    pub fn iter(&self) -> impl Iterator<Item = Result<OutputChannel, Jpeg2000Error>> + '_ {
        (0..self.count()).map(|index| self.channel(index))
    }

    /// Returns the first opacity channel, which a PDF soft mask reads.
    ///
    /// `SMaskInData` selects between an ordinary and a premultiplied opacity
    /// channel, so the kind is carried through rather than collapsed here.
    ///
    /// # Errors
    ///
    /// Propagates a malformed channel chain from [`Self::channel`].
    pub fn opacity(&self) -> Result<Option<OutputChannel>, Jpeg2000Error> {
        for channel in self.iter() {
            let channel = channel?;
            if channel.is_opacity() {
                return Ok(Some(channel));
            }
        }
        Ok(None)
    }

    /// Returns the colour channels in colour-space order.
    ///
    /// # Errors
    ///
    /// Propagates a malformed channel chain from [`Self::channel`].
    pub fn color_channel_count(&self) -> Result<u16, Jpeg2000Error> {
        let mut count = 0u16;
        for channel in self.iter() {
            if matches!(channel?.kind, ChannelKind::Color) {
                count = count.saturating_add(1);
            }
        }
        Ok(count)
    }

    /// Converts one reconstructed component sample into a channel sample.
    ///
    /// A direct channel passes its sample through. A palette channel treats
    /// the sample as a table index, which Part 1 requires to be an unsigned
    /// value inside the table.
    ///
    /// # Errors
    ///
    /// Returns `Container` when the sample indexes outside the palette.
    pub fn sample(&self, channel: OutputChannel, value: i64) -> Result<i64, Jpeg2000Error> {
        let ChannelSource::Palette { column, .. } = channel.source else {
            return Ok(value);
        };
        let palette = self.palette.ok_or(ContainerError::Channel {
            channel: channel.index,
        })?;
        u16::try_from(value)
            .ok()
            .and_then(|entry| palette.sample(entry, column))
            .ok_or_else(|| {
                ContainerError::PaletteIndex {
                    index: value,
                    entries: palette.entry_count(),
                }
                .into()
            })
    }

    /// Returns the precision and signedness a channel's samples arrive in.
    fn sample_layout(&self, source: ChannelSource) -> Result<(u8, bool), Jpeg2000Error> {
        match source {
            ChannelSource::Direct { component } => {
                let info = self.size.component(component)?;
                Ok((info.precision, info.signed))
            }
            ChannelSource::Palette { column, .. } => {
                let layout = self
                    .palette
                    .and_then(|table| table.column(column))
                    .ok_or(ContainerError::Required(BoxKind::Palette))?;
                Ok((layout.precision, layout.signed))
            }
        }
    }

    /// Names what a channel without a Channel Definition record carries.
    ///
    /// Annex I leaves such a channel undescribed; treating it as the next
    /// colour channel matches the box's own absence. A JPX compositing layer
    /// may instead declare its last channel as opacity, which applies to the
    /// image as a whole.
    fn undescribed_channel(&self, index: u16) -> (ChannelKind, ChannelAssociation) {
        let count = self.count();
        match self.opacity {
            Some(kind) if count > 1 && index == count.saturating_sub(1) => {
                (kind, ChannelAssociation::WholeImage)
            }
            _ => (ChannelKind::Color, Self::default_association(index)),
        }
    }

    /// Names the colour channel an undescribed channel defaults to.
    ///
    /// Channel `n` is colour channel `n + 1` of the image's colour space.
    fn default_association(index: u16) -> ChannelAssociation {
        ChannelAssociation::Color {
            index: index.saturating_add(1),
        }
    }
}
