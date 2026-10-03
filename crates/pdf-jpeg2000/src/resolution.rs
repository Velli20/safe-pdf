//! Resolution levels and the subbands of one tile-component.
//!
//! Annex B.5 derives resolution level `r` of a tile-component by halving its
//! region `NL - r` times, and derives the subbands of each level from the same
//! region with the `(xob, yob)` offsets of Table B.1. Level zero holds the one
//! `LL` band left by the decomposition; every higher level holds `HL`, `LH`,
//! and `HH`.

use pdf_graphics::point::Point;

use crate::{Jpeg2000Error, region::Region};

/// Subbands carried by any resolution level above level zero.
const HIGHER_LEVEL_BANDS: usize = 3;
/// Quantization step-size entries consumed by one decomposition level.
const STEPS_PER_LEVEL: usize = 3;

/// Orientation of a subband within its decomposition level.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum BandKind {
    /// Low-pass on both axes, the residual of the decomposition.
    #[default]
    Ll,
    /// High-pass horizontally, low-pass vertically.
    Hl,
    /// Low-pass horizontally, high-pass vertically.
    Lh,
    /// High-pass on both axes.
    Hh,
}

impl BandKind {
    /// Returns the Table B.1 `(xob, yob)` offsets of this orientation.
    fn offset(self) -> Point<u32> {
        let (x, y) = match self {
            Self::Ll => (0, 0),
            Self::Hl => (1, 0),
            Self::Lh => (0, 1),
            Self::Hh => (1, 1),
        };
        Point { x, y }
    }

    /// Returns the Table E.1 log-two gain applied by inverse quantization.
    pub(crate) fn gain(self) -> u8 {
        match self {
            Self::Ll => 0,
            Self::Hl | Self::Lh => 1,
            Self::Hh => 2,
        }
    }
}

/// One subband of a resolution level, on the subband coefficient grid.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Subband {
    /// Orientation of the band within its decomposition level.
    pub(crate) kind: BandKind,
    /// Coefficient extent of the band.
    pub(crate) region: Region,
    /// Decompositions applied above the band, the `nb` of Annex B.5.
    pub(crate) decomposition: u32,
    /// Index of the band's entry in a QCD or QCC step-size table.
    pub(crate) step_index: usize,
}

/// The subbands of one resolution level, held without allocating.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Bands {
    entries: [Subband; HIGHER_LEVEL_BANDS],
    count: usize,
}

impl Bands {
    /// Returns the bands of the level in codestream order.
    pub(crate) fn as_slice(&self) -> &[Subband] {
        self.entries.get(..self.count).unwrap_or_default()
    }
}

/// One resolution level of a tile-component.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Resolution {
    index: u8,
    levels: u8,
    component: Region,
    region: Region,
}

impl Resolution {
    /// Derives resolution level `index` of a tile-component.
    ///
    /// # Errors
    ///
    /// Returns `Overflow` if the level is above the component's decomposition
    /// count or its region cannot be represented.
    pub(crate) fn new(component: Region, levels: u8, index: u8) -> Result<Self, Jpeg2000Error> {
        let remaining = levels.checked_sub(index).ok_or(Jpeg2000Error::Overflow {
            context: "resolution level index",
        })?;
        Ok(Self {
            index,
            levels,
            component,
            region: component.downscale(u32::from(remaining))?,
        })
    }

    /// Returns the zero-based resolution level.
    pub(crate) fn index(self) -> u8 {
        self.index
    }

    /// Returns the level's extent on the resolution grid.
    pub(crate) fn region(self) -> Region {
        self.region
    }

    /// Returns whether this level is the residual `LL` band.
    fn is_residual(self) -> bool {
        self.index == 0
    }

    /// Returns the decompositions applied above this level's subbands.
    fn decomposition(self) -> u32 {
        let levels = u32::from(self.levels);
        if self.is_residual() {
            return levels;
        }
        levels
            .saturating_sub(u32::from(self.index))
            .saturating_add(1)
    }

    /// Returns the subbands the level contributes to the tile-component.
    ///
    /// # Errors
    ///
    /// Returns `Overflow` if a band region cannot be represented.
    pub(crate) fn bands(self) -> Result<Bands, Jpeg2000Error> {
        let decomposition = self.decomposition();
        let mut bands = Bands::default();
        let kinds: &[BandKind] = if self.is_residual() {
            &[BandKind::Ll]
        } else {
            &[BandKind::Hl, BandKind::Lh, BandKind::Hh]
        };
        for (slot, kind) in bands.entries.iter_mut().zip(kinds) {
            *slot = Subband {
                kind: *kind,
                region: self.component.subband(decomposition, kind.offset())?,
                decomposition,
                step_index: self.step_index(*kind)?,
            };
        }
        bands.count = kinds.len();
        Ok(bands)
    }

    /// Returns a band's position in the QCD or QCC step-size table.
    ///
    /// The table lists the residual `LL` band first, then `HL`, `LH`, and `HH`
    /// for each decomposition level from the coarsest upwards.
    fn step_index(self, kind: BandKind) -> Result<usize, Jpeg2000Error> {
        let overflow = || Jpeg2000Error::Overflow {
            context: "quantization step index",
        };
        if self.is_residual() {
            return Ok(0);
        }
        let position = match kind {
            BandKind::Ll => return Err(overflow()),
            BandKind::Hl => 1,
            BandKind::Lh => 2,
            BandKind::Hh => 3,
        };
        usize::from(self.index)
            .checked_sub(1)
            .and_then(|level| level.checked_mul(STEPS_PER_LEVEL))
            .and_then(|offset| offset.checked_add(position))
            .ok_or_else(overflow)
    }
}
