//! Accounting for the decoder's own temporary storage.
//!
//! Compressed bytes stay borrowed, but packet indexes, tag trees, coefficient
//! planes, and wavelet scratch are owned by the decoder. Every such allocation
//! is charged here against [`DecoderLimits::max_working_bytes`] before it is
//! requested from the allocator, so a hostile image is rejected with
//! `LimitExceeded` instead of exhausting memory.
//!
//! Storage scoped to one tile is released by taking a [`Mark`] on entry and
//! restoring it on exit, which keeps the budget flat across a multi-tile image
//! without relying on drop order.

use crate::{Jpeg2000Error, Resource, limits::DecoderLimits};

/// A saved budget position that later allocations can be rolled back to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Mark(usize);

/// Tracks decoder-owned bytes against the caller's working-memory bound.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Workspace {
    limit: usize,
    used: usize,
}

impl Workspace {
    /// Creates an empty budget from the caller's limits.
    pub(crate) fn new(limits: DecoderLimits) -> Self {
        Self {
            limit: limits.max_working_bytes,
            used: 0,
        }
    }

    /// Returns the current budget position for a later [`Workspace::release`].
    pub(crate) fn mark(&self) -> Mark {
        Mark(self.used)
    }

    /// Returns the budget to a position taken before a scoped allocation.
    pub(crate) fn release(&mut self, mark: Mark) {
        self.used = self.used.min(mark.0);
    }

    /// Charges bytes that are about to be allocated.
    ///
    /// # Errors
    ///
    /// Returns `LimitExceeded` when the request passes the caller's bound.
    pub(crate) fn charge(&mut self, bytes: usize) -> Result<(), Jpeg2000Error> {
        let used = self
            .used
            .checked_add(bytes)
            .ok_or(Jpeg2000Error::Overflow {
                context: "working memory total",
            })?;
        Resource::WorkingBytes.check_bytes(used, self.limit)?;
        self.used = used;
        Ok(())
    }

    /// Appends one element to a vector, charging its bytes to the budget.
    ///
    /// Growing a list incrementally keeps a packet index bounded without
    /// counting the same sequence twice to size it in advance.
    ///
    /// # Errors
    ///
    /// Returns `LimitExceeded` when the element passes the caller's bound, and
    /// `Overflow` when the allocator cannot satisfy the growth.
    pub(crate) fn push<T>(&mut self, buffer: &mut Vec<T>, value: T) -> Result<(), Jpeg2000Error> {
        self.charge(size_of::<T>())?;
        buffer.try_reserve(1).map_err(|_| Jpeg2000Error::Overflow {
            context: "working buffer allocation",
        })?;
        buffer.push(value);
        Ok(())
    }

    /// Allocates a zeroed vector of `len` elements within the budget.
    ///
    /// # Errors
    ///
    /// Returns `LimitExceeded` when the request passes the caller's bound, and
    /// `Overflow` when the allocator cannot satisfy it.
    pub(crate) fn vector<T: Clone + Default>(
        &mut self,
        len: usize,
    ) -> Result<Vec<T>, Jpeg2000Error> {
        let bytes = len
            .checked_mul(size_of::<T>())
            .ok_or(Jpeg2000Error::Overflow {
                context: "working buffer size",
            })?;
        self.charge(bytes)?;
        let mut buffer = Vec::new();
        buffer
            .try_reserve_exact(len)
            .map_err(|_| Jpeg2000Error::Overflow {
                context: "working buffer allocation",
            })?;
        buffer.resize(len, T::default());
        Ok(buffer)
    }
}
