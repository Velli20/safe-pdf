//! Dimension-aware LRU caching for recorded pages and their text layouts.

use std::collections::{HashMap, VecDeque};

use crate::RecordedPage;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct CacheKey {
    page_index: usize,
    width_bits: u32,
    height_bits: u32,
}

impl CacheKey {
    fn new(page_index: usize, width: f32, height: f32) -> Self {
        Self {
            page_index,
            width_bits: width.to_bits(),
            height_bits: height.to_bits(),
        }
    }
}

/// LRU cache of combined drawing recordings and selectable text layouts.
///
/// Entries are keyed by page index and exact canvas dimensions because both
/// drawing commands and glyph bounds are stored in device coordinates.
pub struct PageRecordingCache {
    recordings: HashMap<CacheKey, RecordedPage>,
    max_entries: usize,
    access_order: VecDeque<CacheKey>,
}

impl PageRecordingCache {
    /// Creates a cache retaining at most `max_entries` page/size combinations.
    pub fn new(max_entries: usize) -> Self {
        Self {
            recordings: HashMap::with_capacity(max_entries),
            max_entries,
            access_order: VecDeque::with_capacity(max_entries),
        }
    }

    /// Returns the recorded page for an exact page/size combination.
    pub fn get(&mut self, page_index: usize, width: f32, height: f32) -> Option<&RecordedPage> {
        let key = CacheKey::new(page_index, width, height);
        if !self.recordings.contains_key(&key) {
            return None;
        }
        self.access_order.retain(|candidate| *candidate != key);
        self.access_order.push_back(key);
        self.recordings.get(&key)
    }

    /// Inserts a recorded page, deriving its cache dimensions from the recording.
    pub fn insert(&mut self, page_index: usize, recorded_page: RecordedPage) {
        if self.max_entries == 0 {
            return;
        }
        let key = CacheKey::new(
            page_index,
            recorded_page.recording().width,
            recorded_page.recording().height,
        );
        self.access_order.retain(|candidate| *candidate != key);
        while self.recordings.len() >= self.max_entries && !self.recordings.contains_key(&key) {
            let Some(oldest) = self.access_order.pop_front() else {
                break;
            };
            self.recordings.remove(&oldest);
        }
        self.access_order.push_back(key);
        self.recordings.insert(key, recorded_page);
    }

    /// Removes every cached size for `page_index`.
    pub fn remove(&mut self, page_index: usize) -> Vec<RecordedPage> {
        let keys: Vec<CacheKey> = self
            .recordings
            .keys()
            .filter(|key| key.page_index == page_index)
            .copied()
            .collect();
        self.access_order.retain(|key| key.page_index != page_index);
        keys.into_iter()
            .filter_map(|key| self.recordings.remove(&key))
            .collect()
    }

    /// Clears all cached pages.
    pub fn clear(&mut self) {
        self.recordings.clear();
        self.access_order.clear();
    }

    /// Returns the number of cached page/size combinations.
    pub fn len(&self) -> usize {
        self.recordings.len()
    }

    /// Returns whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.recordings.is_empty()
    }

    /// Returns whether any size of `page_index` is cached.
    pub fn contains(&self, page_index: usize) -> bool {
        self.recordings
            .keys()
            .any(|key| key.page_index == page_index)
    }

    /// Returns uncached neighboring page indices in prefetch order.
    pub fn pages_to_prefetch(&self, current_page: usize, page_count: usize) -> Vec<usize> {
        let mut pages = Vec::new();
        for offset in [1_usize, 2, 3] {
            if let Some(next) = current_page.checked_add(offset)
                && next < page_count
                && !self.contains(next)
            {
                pages.push(next);
            }
            if let Some(previous) = current_page.checked_sub(offset)
                && !self.contains(previous)
            {
                pages.push(previous);
            }
        }
        pages
    }

    /// Returns the maximum number of retained entries.
    pub fn capacity(&self) -> usize {
        self.max_entries
    }
}

impl Default for PageRecordingCache {
    fn default() -> Self {
        Self::new(5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PageTextLayout;
    use pdf_canvas::recording_canvas::RecordingCanvas;

    fn recorded(width: f32, height: f32) -> RecordedPage {
        RecordedPage {
            recording: RecordingCanvas::new(width, height),
            text_layout: PageTextLayout::default(),
        }
    }

    #[test]
    fn cache_keys_entries_by_dimensions() {
        let mut cache = PageRecordingCache::new(3);
        cache.insert(0, recorded(100.0, 100.0));

        assert!(cache.get(0, 100.0, 100.0).is_some());
        assert!(cache.get(0, 200.0, 100.0).is_none());
    }

    #[test]
    fn cache_evicts_least_recent_entry() {
        let mut cache = PageRecordingCache::new(2);
        cache.insert(0, recorded(100.0, 100.0));
        cache.insert(1, recorded(100.0, 100.0));
        let _ = cache.get(0, 100.0, 100.0);
        cache.insert(2, recorded(100.0, 100.0));

        assert!(cache.contains(0));
        assert!(!cache.contains(1));
        assert!(cache.contains(2));
    }

    #[test]
    fn remove_drops_every_size_for_page() {
        let mut cache = PageRecordingCache::new(3);
        cache.insert(0, recorded(100.0, 100.0));
        cache.insert(0, recorded(200.0, 100.0));

        assert_eq!(cache.remove(0).len(), 2);
        assert!(!cache.contains(0));
    }

    #[test]
    fn zero_capacity_cache_is_always_empty() {
        let mut cache = PageRecordingCache::new(0);
        cache.insert(0, recorded(100.0, 100.0));
        assert!(cache.is_empty());
    }

    #[test]
    fn pages_to_prefetch_skip_cached_pages() {
        let mut cache = PageRecordingCache::new(5);
        cache.insert(5, recorded(100.0, 100.0));
        let pages = cache.pages_to_prefetch(5, 10);
        assert!(pages.contains(&6));
        assert!(pages.contains(&4));
        assert!(!pages.contains(&5));
    }
}
