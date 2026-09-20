//! Shared pixel-storage accounting and owned reservation lifetimes.

use crate::error::{WebCanvasBackendError as Error, WebResult};
use pdf_graphics::Image;
use std::{cell::Cell, rc::Rc};

/// An owned readback image whose clones share pixels and their accounting lifetime.
#[derive(Clone)]
pub struct BudgetedImage {
    /// Shared, immutable RGBA pixel storage.
    image: Image,
    /// Released only after the last budgeted image clone is dropped.
    _reservation: Rc<Reservation>,
}

impl BudgetedImage {
    /// Associates owned pixels with the reservation acquired before allocating them.
    pub(crate) fn new(image: Image, reservation: Reservation) -> Self {
        Self {
            image,
            _reservation: Rc::new(reservation),
        }
    }

    /// Borrows the pixels while their reservation remains owned by this image.
    pub fn image(&self) -> &Image {
        &self.image
    }
}

/// Shared byte accounting with automatic release through owned reservations.
///
/// This main-thread counter owns no surfaces, so reservations can outlive the pool
/// without retaining its cache or creating a reference cycle. It tracks declared
/// pixel storage; it does not measure browser allocations or allocator overhead.
#[derive(Clone)]
pub(crate) struct Budget(
    /// Counter and immutable limit shared by every clone and reservation.
    Rc<BudgetState>,
);

/// Accounting state shared independently of the surface cache.
struct BudgetState {
    /// Sum of bytes held by all live reservations, including idle pooled surfaces.
    used: Cell<usize>,
    /// Maximum number of accounted pixel bytes that may be reserved concurrently.
    limit: usize,
}

/// An exclusive accounting charge released on drop, including early error returns.
#[must_use = "dropping the reservation immediately releases its byte charge"]
pub(crate) struct Reservation {
    /// Counter to credit when this reservation is released.
    budget: Budget,
    /// Bytes charged once at construction and released once on drop.
    bytes: usize,
}

impl Budget {
    /// Creates an empty shared counter with the supplied pixel-byte limit.
    pub(crate) fn new(limit: usize) -> Self {
        Self(Rc::new(BudgetState {
            used: Cell::new(0),
            limit,
        }))
    }

    /// Returns the configured maximum number of accounted pixel bytes.
    pub(crate) fn limit(&self) -> usize {
        self.0.limit
    }

    /// Returns live accounted bytes for resource-lifetime assertions.
    #[cfg(test)]
    pub(crate) fn used(&self) -> usize {
        self.0.used.get()
    }

    /// Reserves bytes against the aggregate temporary-storage limit.
    pub(crate) fn reserve(&self, bytes: usize) -> WebResult<Reservation> {
        let used = self
            .0
            .used
            .get()
            .checked_add(bytes)
            .filter(|v| *v <= self.0.limit)
            .ok_or(Error::ResourceLimit)?;
        self.0.used.set(used);
        Ok(Reservation {
            budget: self.clone(),
            bytes,
        })
    }
}

impl Drop for Reservation {
    /// Releases this resource’s reservation from aggregate byte accounting.
    fn drop(&mut self) {
        debug_assert!(self.budget.0.used.get() >= self.bytes);
        self.budget
            .0
            .used
            .set(self.budget.0.used.get().saturating_sub(self.bytes));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Clones share accounting, and rejected requests leave it unchanged.
    fn nested_allocations_share_budget() {
        let budget = Budget::new(16);
        let nested = budget.clone();
        let first = budget.reserve(12).unwrap();
        assert!(nested.reserve(5).is_err());
        assert_eq!(budget.0.used.get(), 12);
        drop(first);
        assert!(nested.reserve(16).is_ok());
    }

    #[test]
    fn early_error_releases_reservations() {
        fn allocate(budget: &Budget) -> WebResult<()> {
            let _first = budget.reserve(12)?;
            let _second = budget.reserve(5)?;
            Ok(())
        }
        let budget = Budget::new(16);
        assert!(allocate(&budget).is_err());
        assert_eq!(budget.0.used.get(), 0);
        assert!(budget.reserve(16).is_ok());
    }

    #[test]
    fn overflow_does_not_change_accounting() {
        let budget = Budget::new(usize::MAX);
        let reservation = budget.reserve(usize::MAX).unwrap();
        assert!(budget.reserve(1).is_err());
        assert_eq!(budget.0.used.get(), usize::MAX);
        drop(reservation);
        assert_eq!(budget.0.used.get(), 0);
    }

    #[test]
    fn reservations_release_in_any_order() {
        for reverse in [false, true] {
            let budget = Budget::new(16);
            let first = budget.reserve(5).unwrap();
            let second = budget.reserve(11).unwrap();
            let (early, late, remaining) = if reverse {
                (second, first, 5)
            } else {
                (first, second, 11)
            };
            drop(early);
            assert_eq!(budget.0.used.get(), remaining);
            drop(late);
            assert_eq!(budget.0.used.get(), 0);
        }
    }
}
