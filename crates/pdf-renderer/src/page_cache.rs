//! Page caching using [`RecordingCanvas`] for efficient re-rendering.
//!
//! This module provides an LRU cache for storing pre-recorded PDF page drawing
//! commands. Since [`RecordingCanvas`] stores resolution-independent vector
//! commands, cached pages can be replayed to any backend at any size without
//! re-parsing the PDF content stream.
//!
//! # Example
//!
//! ```ignore
//! use pdf_renderer::{page_cache::PageRecordingCache, PdfRenderer};
//!
//! let mut cache = PageRecordingCache::new(5); // Cache up to 5 pages
//! let renderer = PdfRenderer::new(document);
//!
//! // Check if page is cached
//! if let Some(recording) = cache.get(page_index) {
//!     recording.replay(&mut backend)?;
//! } else {
//!     // Render to RecordingCanvas and cache it
//!     let recording = renderer.render_page_to_recording(page_index, width, height)?;
//!     cache.insert(page_index, recording.clone());
//!     recording.replay(&mut backend)?;
//! }
//! ```

use pdf_canvas::recording_canvas::RecordingCanvas;
use std::collections::HashMap;

/// LRU cache for storing pre-recorded PDF page drawing commands.
///
/// `PageRecordingCache` stores [`RecordingCanvas`] instances keyed by page index.
/// When the cache reaches its capacity, the least recently accessed page is evicted.
///
/// # Benefits
///
/// - **Resolution-independent**: Cached commands can be replayed at any zoom level.
/// - **Memory efficient**: Stores vector commands instead of pixel data.
/// - **Backend agnostic**: Same cache works for Skia, FemtoVG, or any `CanvasBackend`.
pub struct PageRecordingCache {
    /// Maps page_index -> recorded drawing commands.
    recordings: HashMap<usize, RecordingCanvas>,
    /// Maximum number of pages to cache.
    max_entries: usize,
    /// Access order for LRU eviction (most recent at back).
    access_order: Vec<usize>,
}

impl PageRecordingCache {
    /// Creates a new page cache with the specified capacity.
    ///
    /// # Parameters
    ///
    /// - `max_entries`: Maximum number of pages to keep in the cache.
    ///   When this limit is reached, the least recently accessed page is evicted.
    pub fn new(max_entries: usize) -> Self {
        Self {
            recordings: HashMap::with_capacity(max_entries),
            max_entries,
            access_order: Vec::with_capacity(max_entries),
        }
    }

    /// Gets a cached page recording, updating LRU order.
    ///
    /// # Parameters
    ///
    /// - `page_index`: Zero-based index of the page to retrieve.
    ///
    /// # Returns
    ///
    /// `Some(&RecordingCanvas)` if the page is cached, `None` otherwise.
    pub fn get(&mut self, page_index: usize) -> Option<&RecordingCanvas> {
        if self.recordings.contains_key(&page_index) {
            // Move to end of access order (most recently used)
            self.access_order.retain(|&i| i != page_index);
            self.access_order.push(page_index);
            self.recordings.get(&page_index)
        } else {
            None
        }
    }

    /// Inserts a recorded page into the cache.
    ///
    /// If the cache is at capacity, the least recently accessed page is evicted
    /// before inserting the new one.
    ///
    /// # Parameters
    ///
    /// - `page_index`: Zero-based index of the page.
    /// - `recording`: The recorded drawing commands for the page.
    pub fn insert(&mut self, page_index: usize, recording: RecordingCanvas) {
        // A zero-capacity cache never stores anything.
        if self.max_entries == 0 {
            return;
        }

        // If already present, just update the recording and access order
        if self.recordings.contains_key(&page_index) {
            self.access_order.retain(|&i| i != page_index);
            self.access_order.push(page_index);
            self.recordings.insert(page_index, recording);
            return;
        }

        // Evict oldest entries if at capacity
        while self.recordings.len() >= self.max_entries && !self.access_order.is_empty() {
            if let Some(oldest) = self.access_order.first().copied() {
                self.recordings.remove(&oldest);
                self.access_order.remove(0);
            }
        }

        self.recordings.insert(page_index, recording);
        self.access_order.push(page_index);
    }

    /// Removes a specific page from the cache.
    ///
    /// # Parameters
    ///
    /// - `page_index`: Zero-based index of the page to remove.
    ///
    /// # Returns
    ///
    /// The removed `RecordingCanvas` if it was cached, `None` otherwise.
    pub fn remove(&mut self, page_index: usize) -> Option<RecordingCanvas> {
        self.access_order.retain(|&i| i != page_index);
        self.recordings.remove(&page_index)
    }

    /// Clears all cached pages.
    ///
    /// Call this when loading a new document or when memory needs to be freed.
    pub fn clear(&mut self) {
        self.recordings.clear();
        self.access_order.clear();
    }

    /// Returns the number of pages currently in the cache.
    pub fn len(&self) -> usize {
        self.recordings.len()
    }

    /// Returns `true` if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.recordings.is_empty()
    }

    /// Returns `true` if the specified page is cached.
    ///
    /// Note: This does not update the LRU order. Use [`get`](Self::get) if you
    /// intend to access the cached data.
    pub fn contains(&self, page_index: usize) -> bool {
        self.recordings.contains_key(&page_index)
    }

    /// Returns page indices that should be prefetched for smooth navigation.
    ///
    /// Given the current page, this returns adjacent pages that are not yet
    /// cached, prioritized by distance from the current page.
    ///
    /// # Parameters
    ///
    /// - `current_page`: The currently visible page index.
    /// - `page_count`: Total number of pages in the document.
    ///
    /// # Returns
    ///
    /// A vector of page indices to prefetch, ordered by priority.
    pub fn pages_to_prefetch(&self, current_page: usize, page_count: usize) -> Vec<usize> {
        let mut pages = Vec::new();

        // Prioritize: next page, previous page, then further pages
        for offset in [1i32, -1, 2, -2, 3, -3] {
            let idx = current_page as i32 + offset;
            if idx >= 0 && (idx as usize) < page_count {
                let page_idx = idx as usize;
                if !self.recordings.contains_key(&page_idx) {
                    pages.push(page_idx);
                }
            }
        }

        pages
    }

    /// Returns the maximum capacity of the cache.
    pub fn capacity(&self) -> usize {
        self.max_entries
    }
}

impl Default for PageRecordingCache {
    /// Creates a cache with a default capacity of 5 pages.
    fn default() -> Self {
        Self::new(5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_insert_and_get() {
        let mut cache = PageRecordingCache::new(3);
        let recording = RecordingCanvas::new(100.0, 100.0);

        cache.insert(0, recording.clone());
        assert!(cache.contains(0));
        assert!(cache.get(0).is_some());
    }

    #[test]
    fn test_cache_lru_eviction() {
        let mut cache = PageRecordingCache::new(2);

        cache.insert(0, RecordingCanvas::new(100.0, 100.0));
        cache.insert(1, RecordingCanvas::new(100.0, 100.0));

        // Access page 0 to make it more recent
        let _ = cache.get(0);

        // Insert page 2 - should evict page 1 (least recently used)
        cache.insert(2, RecordingCanvas::new(100.0, 100.0));

        assert!(cache.contains(0));
        assert!(!cache.contains(1)); // Evicted
        assert!(cache.contains(2));
    }

    #[test]
    fn test_pages_to_prefetch() {
        let mut cache = PageRecordingCache::new(5);
        cache.insert(5, RecordingCanvas::new(100.0, 100.0));

        let to_prefetch = cache.pages_to_prefetch(5, 10);

        // Should suggest pages 6, 4, 7, 3, 8, 2 (adjacent pages not in cache)
        assert!(to_prefetch.contains(&6));
        assert!(to_prefetch.contains(&4));
        assert!(!to_prefetch.contains(&5)); // Already cached
    }

    #[test]
    fn test_zero_capacity_cache_is_always_empty() {
        let mut cache = PageRecordingCache::new(0);

        cache.insert(0, RecordingCanvas::new(100.0, 100.0));
        cache.insert(1, RecordingCanvas::new(200.0, 200.0));

        assert!(cache.is_empty());
        assert!(!cache.contains(0));
        assert!(!cache.contains(1));
        assert_eq!(cache.len(), 0);
        assert!(cache.get(0).is_none());
    }

    #[test]
    fn test_cache_clear() {
        let mut cache = PageRecordingCache::new(3);
        cache.insert(0, RecordingCanvas::new(100.0, 100.0));
        cache.insert(1, RecordingCanvas::new(100.0, 100.0));

        assert_eq!(cache.len(), 2);

        cache.clear();

        assert!(cache.is_empty());
        assert!(!cache.contains(0));
    }
}
