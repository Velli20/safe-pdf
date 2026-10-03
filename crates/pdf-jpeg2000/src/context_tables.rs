//! Context selection for the three code-block coding passes.
//!
//! Annex D.3 chooses an arithmetic context from the significance of a
//! coefficient's eight neighbours. Table D.1 selects the significance
//! context, and reads the neighbourhood differently for each subband
//! orientation; Table D.3 selects the sign context together with a bit that
//! flips the decoded sign; Table D.4 selects the magnitude refinement
//! context.

use crate::{code_block_state::Neighbourhood, mq_contexts::ContextLabel, resolution::BandKind};

/// First sign context label of Table D.3.
const SIGN_BASE: usize = 9;
/// Magnitude refinement context for a first refinement with no neighbours.
const REFINEMENT_ISOLATED: usize = 14;
/// Magnitude refinement context for a first refinement beside a neighbour.
const REFINEMENT_NEIGHBOURED: usize = 15;
/// Magnitude refinement context for every later refinement.
const REFINEMENT_LATER: usize = 16;

/// Returns the Table D.1 significance context of a neighbourhood.
///
/// The table is written for the `LL` and `LH` orientations. `HL` reads the
/// same table with its axes exchanged, because its coefficients are high-pass
/// along the other axis, and `HH` has a table of its own keyed on the
/// diagonal count.
pub(crate) fn significance_context(band: BandKind, neighbours: Neighbourhood) -> ContextLabel {
    let label = match band {
        BandKind::Ll | BandKind::Lh => axis_context(
            neighbours.horizontal,
            neighbours.vertical,
            neighbours.diagonal,
        ),
        BandKind::Hl => axis_context(
            neighbours.vertical,
            neighbours.horizontal,
            neighbours.diagonal,
        ),
        BandKind::Hh => diagonal_context(
            neighbours.diagonal,
            neighbours.horizontal.saturating_add(neighbours.vertical),
        ),
    };
    ContextLabel::new(label).unwrap_or(ContextLabel::UNIFORM)
}

/// Returns the Table D.1 context for an orientation with a primary axis.
///
/// `primary` counts the significant neighbours along the axis the subband is
/// low-pass in, and `secondary` those along the other axis.
fn axis_context(primary: u8, secondary: u8, diagonal: u8) -> usize {
    match (primary, secondary, diagonal) {
        (2.., _, _) => 8,
        (1, 1.., _) => 7,
        (1, 0, 1..) => 6,
        (1, 0, 0) => 5,
        (0, 2.., _) => 4,
        (0, 1, _) => 3,
        (0, 0, 2..) => 2,
        (0, 0, 1) => 1,
        (0, 0, 0) => 0,
    }
}

/// Returns the Table D.1 context of an `HH` subband.
fn diagonal_context(diagonal: u8, axes: u8) -> usize {
    match (diagonal, axes) {
        (3.., _) => 8,
        (2, 1..) => 7,
        (2, 0) => 6,
        (1, 2..) => 5,
        (1, 1) => 4,
        (1, 0) => 3,
        (0, 2..) => 2,
        (0, 1) => 1,
        (0, 0) => 0,
    }
}

/// Returns the Table D.3 sign context and the bit that flips the sign.
///
/// The horizontal and vertical sign sums are clamped to one contribution per
/// axis, so the table has nine entries covering the symmetric pairs.
pub(crate) fn sign_context(neighbours: Neighbourhood) -> (ContextLabel, bool) {
    let horizontal = neighbours.horizontal_sign.clamp(-1, 1);
    let vertical = neighbours.vertical_sign.clamp(-1, 1);
    let (offset, flip) = match (horizontal, vertical) {
        (1, 1) => (4, false),
        (1, 0) => (3, false),
        (1, _) => (2, false),
        (0, 1) => (1, false),
        (0, 0) => (0, false),
        (0, _) => (1, true),
        (_, 1) => (2, true),
        (_, 0) => (3, true),
        (_, _) => (4, true),
    };
    let label =
        ContextLabel::new(SIGN_BASE.saturating_add(offset)).unwrap_or(ContextLabel::UNIFORM);
    (label, flip)
}

/// Returns the Table D.4 magnitude refinement context.
///
/// The neighbourhood only matters for a coefficient's first refinement; after
/// that the magnitude bits are close enough to equiprobable that Annex D uses
/// a single context.
pub(crate) fn refinement_context(first: bool, neighbours: Neighbourhood) -> ContextLabel {
    let label = match (first, neighbours.any_significant()) {
        (false, _) => REFINEMENT_LATER,
        (true, false) => REFINEMENT_ISOLATED,
        (true, true) => REFINEMENT_NEIGHBOURED,
    };
    ContextLabel::new(label).unwrap_or(ContextLabel::UNIFORM)
}
