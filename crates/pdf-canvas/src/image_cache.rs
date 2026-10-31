use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use pdf_graphics::Image;
use pdf_resources::image_xobject::ImageXObject;

use crate::error::PdfCanvasError;

/// Byte budget of [`ImageCache::default`].
pub const DEFAULT_IMAGE_CACHE_BUDGET: usize = 128 * 1024 * 1024;

/// Decoded image XObject rasters shared by the renders that draw them.
///
/// Entries are keyed by the address of the image resource, which each entry keeps
/// alive. Once the rasters exceed the byte budget, the least recently drawn ones are
/// released, so repeated renders of a page decode its images once while memory stays
/// bounded however many pages are drawn.
#[derive(Debug)]
pub struct ImageCache {
    budget: usize,
    entries: Mutex<ImageCacheEntries>,
}

/// The cached rasters and the bookkeeping that orders their eviction.
#[derive(Debug, Default)]
struct ImageCacheEntries {
    map: HashMap<usize, CachedImage>,
    /// Bytes held by the rasters in `map`.
    size: usize,
    /// Use counter stamped on an entry each time it is drawn.
    clock: u64,
}

/// One decoded image; `raster` is `None` when its data cannot be decoded.
#[derive(Debug)]
struct CachedImage {
    _image: Arc<ImageXObject>,
    raster: Option<Arc<Image>>,
    last_use: u64,
}

impl CachedImage {
    /// Bytes the raster holds.
    fn size(&self) -> usize {
        self.raster.as_ref().map_or(0, |raster| raster.data.len())
    }
}

impl Default for ImageCache {
    fn default() -> Self {
        Self::new(DEFAULT_IMAGE_CACHE_BUDGET)
    }
}

impl ImageCache {
    /// Creates an empty cache that holds at most `budget` bytes of rasters.
    pub fn new(budget: usize) -> Self {
        Self {
            budget,
            entries: Mutex::default(),
        }
    }

    /// Returns the raster of `image`, decoding it unless it is cached.
    ///
    /// Returns `None` when the image data cannot be decoded. A raster larger than the
    /// whole budget is returned without being kept.
    pub(crate) fn get_or_decode(
        &self,
        image: &Arc<ImageXObject>,
    ) -> Result<Option<Arc<Image>>, PdfCanvasError> {
        let key = Arc::as_ptr(image).addr();
        if let Some(raster) = self.lock().touch(key) {
            return Ok(raster);
        }
        // Decoding runs unlocked, so other renders keep reading the cache meanwhile.
        let raster = image
            .decode()
            .map_err(|e| PdfCanvasError::InvalidImageData(e.to_string()))?
            .map(Arc::new);
        let entry = CachedImage {
            _image: Arc::clone(image),
            raster: raster.clone(),
            last_use: 0,
        };
        if entry.size() <= self.budget {
            self.lock().insert(key, entry, self.budget);
        }
        Ok(raster)
    }

    /// Locks the entries; a render that failed while holding the lock left them
    /// consistent, because each update completes before it unlocks.
    fn lock(&self) -> std::sync::MutexGuard<'_, ImageCacheEntries> {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl ImageCacheEntries {
    /// Advances the use counter.
    fn tick(&mut self) -> u64 {
        self.clock = self.clock.wrapping_add(1);
        self.clock
    }

    /// Returns the cached raster for `key`, marking it as just drawn.
    fn touch(&mut self, key: usize) -> Option<Option<Arc<Image>>> {
        let now = self.tick();
        let entry = self.map.get_mut(&key)?;
        entry.last_use = now;
        Some(entry.raster.clone())
    }

    /// Stores `entry`, then releases the least recently drawn rasters until the
    /// cache fits in `budget`.
    fn insert(&mut self, key: usize, mut entry: CachedImage, budget: usize) {
        entry.last_use = self.tick();
        self.size = self.size.saturating_add(entry.size());
        if let Some(previous) = self.map.insert(key, entry) {
            self.size = self.size.saturating_sub(previous.size());
        }
        while self.size > budget {
            let Some(oldest) = self
                .map
                .iter()
                .min_by_key(|(_, entry)| entry.last_use)
                .map(|(key, _)| *key)
            else {
                break;
            };
            if let Some(evicted) = self.map.remove(&oldest) {
                self.size = self.size.saturating_sub(evicted.size());
            }
        }
    }
}
