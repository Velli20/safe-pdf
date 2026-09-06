/// The content-stream ID allocator has exhausted its available values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("content stream ID allocator exhausted available usize values")]
pub struct ContentStreamIdExhausted;

/// Allocates monotonically increasing IDs for parsed content streams.
#[derive(Debug, Default)]
pub struct ContentStreamIdAllocator {
    next_id: std::sync::atomic::AtomicUsize,
}

impl ContentStreamIdAllocator {
    /// Creates a new allocator whose first issued ID is `0`.
    pub const fn new() -> Self {
        Self {
            next_id: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// Returns the next content-stream ID.
    ///
    /// # Errors
    ///
    /// Returns [`ContentStreamIdExhausted`] if the allocator
    /// cannot produce another `usize` ID.
    pub fn next_id(&self) -> Result<usize, ContentStreamIdExhausted> {
        self.next_id
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |next| next.checked_add(1),
            )
            .map_err(|_| ContentStreamIdExhausted)
    }
}

#[cfg(test)]
mod tests {
    use super::ContentStreamIdAllocator;

    #[test]
    fn id_allocator_starts_at_zero() {
        let ids = ContentStreamIdAllocator::new();
        assert_eq!(ids.next_id().expect("first id should allocate"), 0);
        assert_eq!(ids.next_id().expect("second id should allocate"), 1);
        assert_eq!(ids.next_id().expect("third id should allocate"), 2);
    }
}
