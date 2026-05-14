use pdf_content_stream_operators::error::PdfOperatorError;

/// Allocates monotonically increasing IDs for parsed content streams.
#[derive(Debug, Default)]
pub struct ContentStreamIdAllocator {
    next_id: usize,
}

impl ContentStreamIdAllocator {
    /// Creates a new allocator whose first issued ID is `0`.
    pub const fn new() -> Self {
        Self { next_id: 0 }
    }

    /// Returns the next content-stream ID.
    ///
    /// # Errors
    ///
    /// Returns [`PdfOperatorError::ContentStreamIdExhausted`] if the allocator
    /// cannot produce another `usize` ID.
    pub fn next_id(&mut self) -> Result<usize, PdfOperatorError> {
        let Some(next_id) = self.next_id.checked_add(1) else {
            return Err(PdfOperatorError::ContentStreamIdExhausted);
        };

        let id = self.next_id;
        self.next_id = next_id;
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::ContentStreamIdAllocator;

    #[test]
    fn id_allocator_starts_at_zero() {
        let mut ids = ContentStreamIdAllocator::new();
        assert_eq!(ids.next_id().expect("first id should allocate"), 0);
        assert_eq!(ids.next_id().expect("second id should allocate"), 1);
        assert_eq!(ids.next_id().expect("third id should allocate"), 2);
    }
}
