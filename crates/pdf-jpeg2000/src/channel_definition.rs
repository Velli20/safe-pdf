//! The JP2 Channel Definition box.
//!
//! Part 1 I.5.3.6 gives each output channel a meaning and an association: a
//! colour channel names the colour-space channel it carries, while an opacity
//! channel names the channels it applies to. A PDF renderer needs this to tell
//! an alpha channel from a fourth colour channel and to know whether the
//! colour samples were premultiplied.

use crate::{box_reader::Jp2Box, jp2::ContainerError};

/// Bytes in one channel definition record.
const RECORD_BYTES: usize = 6;
/// `Typ` value for a colour channel.
const COLOR: u16 = 0;
/// `Typ` value for an opacity channel.
const OPACITY: u16 = 1;
/// `Typ` value for an opacity channel whose colour is premultiplied.
const PREMULTIPLIED_OPACITY: u16 = 2;
/// `Typ` and `Asoc` value meaning the field carries no information.
const UNSPECIFIED: u16 = 0xffff;
/// `Asoc` value associating a channel with the whole image.
const WHOLE_IMAGE: u16 = 0;

/// What one output channel carries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ChannelKind {
    /// A colour channel of the image's colour space.
    Color,
    /// An opacity channel applied to colour samples as they stand.
    Opacity,
    /// An opacity channel whose colour samples were multiplied by it.
    PremultipliedOpacity,
    /// The file declines to say what the channel carries.
    Unspecified,
}

impl From<u16> for ChannelKind {
    /// Maps a `Typ` field, treating a reserved value as unspecified.
    fn from(kind: u16) -> Self {
        match kind {
            COLOR => Self::Color,
            OPACITY => Self::Opacity,
            PREMULTIPLIED_OPACITY => Self::PremultipliedOpacity,
            _ => Self::Unspecified,
        }
    }
}

/// What one output channel applies to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ChannelAssociation {
    /// The channel applies to the image as a whole.
    WholeImage,
    /// The channel carries or applies to one colour-space channel.
    Color {
        /// One-based index of the colour channel in the colour space.
        index: u16,
    },
    /// The file declines to say what the channel applies to.
    Unspecified,
}

impl From<u16> for ChannelAssociation {
    /// Maps an `Asoc` field to the channel it names.
    fn from(association: u16) -> Self {
        match association {
            WHOLE_IMAGE => Self::WholeImage,
            UNSPECIFIED => Self::Unspecified,
            index => Self::Color { index },
        }
    }
}

/// One record of the Channel Definition box.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChannelDefinition {
    /// Zero-based index of the output channel this record describes.
    pub channel: u16,
    /// What the channel carries.
    pub kind: ChannelKind,
    /// What the channel applies to.
    pub association: ChannelAssociation,
}

/// A validated JP2 Channel Definition box whose records remain borrowed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChannelDefinitions<'a> {
    records: &'a [[u8; RECORD_BYTES]],
}

impl<'a> TryFrom<Jp2Box<'a>> for ChannelDefinitions<'a> {
    type Error = ContainerError;

    /// Checks the declared record count against the payload length.
    fn try_from(box_view: Jp2Box<'a>) -> Result<Self, Self::Error> {
        let [count_hi, count_lo, body @ ..] = box_view.payload() else {
            return Err(box_view.into());
        };
        let count: usize = u16::from_be_bytes([*count_hi, *count_lo]).into();
        let (records, remainder) = body.as_chunks::<RECORD_BYTES>();
        if count == 0 || count != records.len() || !remainder.is_empty() {
            return Err(box_view.into());
        }
        Ok(Self { records })
    }
}

impl<'a> ChannelDefinitions<'a> {
    /// Returns the number of channels the box describes.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Returns whether the box describes no channel.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Returns every record in storage order.
    pub fn iter(&self) -> impl Iterator<Item = ChannelDefinition> + 'a {
        self.records.iter().map(
            |&[channel_hi, channel_lo, kind_hi, kind_lo, asoc_hi, asoc_lo]| ChannelDefinition {
                channel: u16::from_be_bytes([channel_hi, channel_lo]),
                kind: u16::from_be_bytes([kind_hi, kind_lo]).into(),
                association: u16::from_be_bytes([asoc_hi, asoc_lo]).into(),
            },
        )
    }

    /// Returns the record describing one output channel, if the box has one.
    pub fn get(&self, channel: u16) -> Option<ChannelDefinition> {
        self.iter().find(|record| record.channel == channel)
    }

    /// Rejects a record naming a channel the component mapping does not define.
    pub(crate) fn validate(
        &self,
        box_view: Jp2Box<'a>,
        channels: u16,
    ) -> Result<(), ContainerError> {
        if self.iter().any(|record| record.channel >= channels) {
            return Err(box_view.into());
        }
        Ok(())
    }
}
