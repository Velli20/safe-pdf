//! The JP2 Component Mapping box.
//!
//! Part 1 I.5.3.5 lists one record per output channel, naming the codestream
//! component it comes from and whether that component is a direct sample or an
//! index into a palette column. Without this box each component is its own
//! channel, which is the ordinary JP2 arrangement.

use crate::{box_reader::Jp2Box, jp2::ContainerError, palette::Palette};

/// Bytes in one component mapping record.
const RECORD_BYTES: usize = 4;
/// `MTYP` value using a codestream component's samples directly.
const DIRECT: u8 = 0;
/// `MTYP` value using a component's samples as palette indices.
const PALETTE_LOOKUP: u8 = 1;

/// Where one output channel's samples come from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ChannelSource {
    /// The samples of one codestream component, used unchanged.
    Direct {
        /// Zero-based SIZ component index.
        component: u16,
    },
    /// One palette column, indexed by a codestream component's samples.
    Palette {
        /// Zero-based SIZ component index supplying the palette index.
        component: u16,
        /// Zero-based palette column read for this channel.
        column: u8,
    },
}

impl ChannelSource {
    /// Returns the codestream component this channel reads.
    pub fn component(self) -> u16 {
        match self {
            Self::Direct { component } | Self::Palette { component, .. } => component,
        }
    }
}

/// A validated JP2 Component Mapping box whose records remain borrowed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComponentMapping<'a> {
    records: &'a [[u8; RECORD_BYTES]],
}

impl<'a> TryFrom<Jp2Box<'a>> for ComponentMapping<'a> {
    type Error = ContainerError;

    /// Checks that the payload is a whole number of records.
    ///
    /// The count is bounded by `u16` so that a channel index shares the width
    /// of the component indices the records name.
    fn try_from(box_view: Jp2Box<'a>) -> Result<Self, Self::Error> {
        let (records, remainder) = box_view.payload().as_chunks::<RECORD_BYTES>();
        if records.is_empty() || !remainder.is_empty() || u16::try_from(records.len()).is_err() {
            return Err(box_view.into());
        }
        Ok(Self { records })
    }
}

impl<'a> ComponentMapping<'a> {
    /// Returns the number of output channels the box defines.
    pub fn channel_count(self) -> u16 {
        // `TryFrom` rejects a box whose record count exceeds this width.
        u16::try_from(self.records.len()).unwrap_or(u16::MAX)
    }

    /// Returns one channel's source, or `None` past the last record.
    pub fn channel(&self, index: u16) -> Option<ChannelSource> {
        let &[hi, lo, kind, column] = self.records.get(usize::from(index))?;
        let component = u16::from_be_bytes([hi, lo]);
        match kind {
            DIRECT => Some(ChannelSource::Direct { component }),
            PALETTE_LOOKUP => Some(ChannelSource::Palette { component, column }),
            _ => None,
        }
    }

    /// Returns every channel's source in record order.
    pub fn channels(&self) -> impl Iterator<Item = Option<ChannelSource>> + '_ {
        (0..self.channel_count()).map(|index| self.channel(index))
    }

    /// Rejects a record naming an absent component, palette, or column.
    ///
    /// The box is validated against the image it belongs to rather than at
    /// parse time, because both the component count and the palette live
    /// outside it.
    pub(crate) fn validate(
        &self,
        box_view: Jp2Box<'a>,
        components: u16,
        palette: Option<&Palette<'a>>,
    ) -> Result<(), ContainerError> {
        let valid = self.channels().all(|source| match source {
            Some(ChannelSource::Direct { component }) => component < components,
            Some(ChannelSource::Palette { component, column }) => {
                component < components && palette.is_some_and(|table| column < table.column_count())
            }
            None => false,
        });
        if !valid {
            return Err(box_view.into());
        }
        Ok(())
    }
}
