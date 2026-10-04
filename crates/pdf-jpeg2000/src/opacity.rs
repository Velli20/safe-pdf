//! The JPX Opacity box.
//!
//! Part 2 Annex M lets a compositing layer declare opacity without a Channel
//! Definition box: the layer's last channel carries opacity, carries
//! premultiplied opacity, or a chroma key names the colour that is transparent.
//! The first two forms describe a channel this decoder already reconstructs;
//! the chroma key is a display rule outside the channel model, so the caller
//! refuses it.

use crate::{box_reader::Jp2Box, channel_definition::ChannelKind, jp2::ContainerError};

/// `OTyp` value giving the last channel as opacity.
const LAST_CHANNEL: u8 = 0;
/// `OTyp` value giving the last channel as premultiplied opacity.
const PREMULTIPLIED_LAST_CHANNEL: u8 = 1;
/// `OTyp` value naming a transparent colour instead of a channel.
const CHROMA_KEY: u8 = 2;

/// How a compositing layer expresses opacity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum OpacityKind {
    /// The layer's last channel carries opacity.
    LastChannel,
    /// The layer's last channel carries opacity, and colour is premultiplied.
    PremultipliedLastChannel,
    /// One colour value is transparent and no channel carries opacity.
    ChromaKey,
}

impl TryFrom<u8> for OpacityKind {
    type Error = u8;

    /// Maps an `OTyp` field, returning an unrecognised value unchanged.
    fn try_from(kind: u8) -> Result<Self, Self::Error> {
        match kind {
            LAST_CHANNEL => Ok(Self::LastChannel),
            PREMULTIPLIED_LAST_CHANNEL => Ok(Self::PremultipliedLastChannel),
            CHROMA_KEY => Ok(Self::ChromaKey),
            unknown => Err(unknown),
        }
    }
}

/// A validated Opacity box whose chroma-key values remain borrowed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Opacity<'a> {
    kind: OpacityKind,
    chroma_key: &'a [u8],
}

impl<'a> TryFrom<Jp2Box<'a>> for Opacity<'a> {
    type Error = ContainerError;

    /// Reads `OTyp` and, for a chroma key, its counted list of values.
    ///
    /// A chroma-key value is as wide as the channel it applies to, so the
    /// values stay borrowed and are not split into samples here.
    fn try_from(box_view: Jp2Box<'a>) -> Result<Self, Self::Error> {
        let invalid = || ContainerError::from(box_view);
        let (kind, body) = box_view.payload().split_first().ok_or_else(invalid)?;
        let kind = OpacityKind::try_from(*kind).map_err(|_| invalid())?;
        let chroma_key = match kind {
            OpacityKind::ChromaKey => {
                let (count, values) = body.split_first().ok_or_else(invalid)?;
                if *count == 0 || values.len() < usize::from(*count) {
                    return Err(invalid());
                }
                values
            }
            OpacityKind::LastChannel | OpacityKind::PremultipliedLastChannel => {
                if !body.is_empty() {
                    return Err(invalid());
                }
                body
            }
        };
        Ok(Self { kind, chroma_key })
    }
}

impl<'a> Opacity<'a> {
    /// Returns how the layer expresses opacity.
    pub fn kind(&self) -> OpacityKind {
        self.kind
    }

    /// Borrows the chroma-key values, which are empty for the other forms.
    pub fn chroma_key(&self) -> &'a [u8] {
        self.chroma_key
    }

    /// Returns what the layer's last channel carries, when a channel does.
    ///
    /// A chroma key names a transparent colour rather than a channel, so it
    /// describes no channel kind.
    pub fn channel_kind(&self) -> Option<ChannelKind> {
        match self.kind {
            OpacityKind::LastChannel => Some(ChannelKind::Opacity),
            OpacityKind::PremultipliedLastChannel => Some(ChannelKind::PremultipliedOpacity),
            OpacityKind::ChromaKey => None,
        }
    }
}
