//! The inverse multiple-component transform and the DC level shift.
//!
//! Annex G.2 and G.3 define two transforms across the first three components
//! of a tile: a reversible one built from integer additions and shifts, and
//! an irreversible one that is the usual luminance and chrominance matrix.
//! Which applies follows from the wavelet filter, so the sample type selects
//! it here as it does for the lifting steps.
//!
//! Annex G.1.2 then returns each component to its declared range: an unsigned
//! component is shifted up by half its range, and every component is clamped
//! to what its precision can hold.

use crate::{Jpeg2000Error, codestream::ComponentInfo, coefficient::Coefficient};

/// Components the multiple-component transform applies to.
const TRANSFORMED_COMPONENTS: usize = 3;
/// Shift dividing by four in the reversible inverse transform.
const REVERSIBLE_SHIFT: u32 = 2;
/// Red coefficient of the second chrominance channel.
const RED_FROM_CR: f64 = 1.402;
/// Green coefficient of the first chrominance channel.
const GREEN_FROM_CB: f64 = -0.344_136;
/// Green coefficient of the second chrominance channel.
const GREEN_FROM_CR: f64 = -0.714_136;
/// Blue coefficient of the first chrominance channel.
const BLUE_FROM_CB: f64 = 1.772;

/// Applies the inverse reversible component transform in place.
///
/// # Errors
///
/// Returns `Overflow` if the three components do not have the same extent.
pub(crate) fn inverse_reversible<C: Coefficient>(
    planes: &mut [Vec<C>],
) -> Result<(), Jpeg2000Error> {
    let [first, second, third] = split(planes)?;
    for index in 0..first.len() {
        let (Some(y), Some(u), Some(v)) = (first.get(index), second.get(index), third.get(index))
        else {
            continue;
        };
        let green = y
            .to_i64()
            .saturating_sub(u.to_i64().saturating_add(v.to_i64()) >> REVERSIBLE_SHIFT);
        let red = v.to_i64().saturating_add(green);
        let blue = u.to_i64().saturating_add(green);
        store(first, index, red)?;
        store(second, index, green)?;
        store(third, index, blue)?;
    }
    Ok(())
}

/// Applies the inverse irreversible component transform in place.
///
/// # Errors
///
/// Returns `Overflow` if the three components do not have the same extent.
pub(crate) fn inverse_irreversible(planes: &mut [Vec<f64>]) -> Result<(), Jpeg2000Error> {
    let [first, second, third] = split(planes)?;
    for index in 0..first.len() {
        let (Some(y), Some(cb), Some(cr)) = (first.get(index), second.get(index), third.get(index))
        else {
            continue;
        };
        let red = cr.mul_add(RED_FROM_CR, *y);
        let green = cb.mul_add(GREEN_FROM_CB, cr.mul_add(GREEN_FROM_CR, *y));
        let blue = cb.mul_add(BLUE_FROM_CB, *y);
        if let Some(slot) = first.get_mut(index) {
            *slot = red;
        }
        if let Some(slot) = second.get_mut(index) {
            *slot = green;
        }
        if let Some(slot) = third.get_mut(index) {
            *slot = blue;
        }
    }
    Ok(())
}

/// Applies the DC level shift and clamps a component to its precision.
pub(crate) fn level_shift<C: Coefficient>(samples: &mut [C], info: ComponentInfo) {
    let precision = u32::from(info.precision).min(C::BITS.saturating_sub(1));
    let half = 1i64.checked_shl(precision.saturating_sub(1)).unwrap_or(0);
    let (shift, low, high) = if info.signed {
        (0, half.checked_neg().unwrap_or(0), half.saturating_sub(1))
    } else {
        (half, 0, half.saturating_mul(2).saturating_sub(1))
    };
    for slot in samples {
        let shifted = slot.to_i64().saturating_add(shift).clamp(low, high);
        *slot = C::from_i64(shifted).unwrap_or_default();
    }
}

/// Splits the first three component planes, checking that they match.
fn split<T>(planes: &mut [Vec<T>]) -> Result<[&mut Vec<T>; TRANSFORMED_COMPONENTS], Jpeg2000Error> {
    let overflow = || Jpeg2000Error::UnsupportedFeature {
        feature: "component transform over components of different sizes",
    };
    let Some((transformed, _)) = planes.split_at_mut_checked(TRANSFORMED_COMPONENTS) else {
        return Err(overflow());
    };
    let [first, second, third] = transformed else {
        return Err(overflow());
    };
    if first.len() != second.len() || second.len() != third.len() {
        return Err(overflow());
    }
    Ok([first, second, third])
}

/// Writes one transformed sample back into a component plane.
fn store<C: Coefficient>(plane: &mut [C], index: usize, value: i64) -> Result<(), Jpeg2000Error> {
    let slot = plane.get_mut(index).ok_or(Jpeg2000Error::Overflow {
        context: "component transform index",
    })?;
    *slot = C::from_i64(value).ok_or(Jpeg2000Error::Overflow {
        context: "component transform range",
    })?;
    Ok(())
}
