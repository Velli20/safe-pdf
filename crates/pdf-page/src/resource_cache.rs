use std::collections::{HashMap, HashSet};

use crate::resource::Resource;

/// A trait for managing cached PDF resources by object number.
///
/// [`ResourceCache`] provides an interface for storing and retrieving PDF
/// resources (such as fonts, images, etc.) associated with their object numbers.
/// This allows efficient reuse and lookup of resources during PDF page processing.
/// Implementors of this trait can define custom caching strategies for resource
/// management.
///
/// The trait also provides cycle-detection hooks (`begin_read`, `end_read`,
/// `is_being_read`) used during recursive resource parsing to break circular
/// references.  Default implementations are no-ops, so existing implementors
/// compile without changes but do not detect cycles.
pub trait ResourceCache {
    /// Retrieves a reference to a `Resource` associated with the given object number,
    /// if it exists.
    ///
    /// # Parameters
    ///
    /// - `obj_num`: The object number used as the key to look up the resource.
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the `Resource` if found, or `None` if
    /// not present.
    fn get(&self, obj_num: &usize) -> Option<&Resource>;

    /// Inserts a `Resource` into the cache, associating it with the given object number.
    ///
    /// # Parameters
    ///
    /// - `obj_num`: The object number to associate with the resource.
    /// - `resource`: The `Resource` to insert into the cache.
    fn insert(&mut self, obj_num: usize, resource: Resource);

    /// Marks `obj_num` as currently being read.
    ///
    /// Returns `true` if the object was **newly** marked (no cycle).
    /// Returns `false` if it was **already** in-progress (cycle detected).
    ///
    /// Used by resource readers to detect circular references in resource
    /// dictionaries (forbidden by ISO 32000-2:2020 §7.8.3).
    fn begin_read(&mut self, _obj_num: usize) -> bool {
        true
    }

    /// Clears the in-progress mark set by [`begin_read`](Self::begin_read).
    fn end_read(&mut self, _obj_num: &usize) {}

    /// Returns `true` if `obj_num` is currently being read in an ancestor
    /// call frame.
    fn is_being_read(&self, _obj_num: &usize) -> bool {
        false
    }
}

impl ResourceCache for HashMap<usize, Resource> {
    fn get(&self, obj_num: &usize) -> Option<&Resource> {
        HashMap::get(self, obj_num)
    }

    fn insert(&mut self, obj_num: usize, resource: Resource) {
        HashMap::insert(self, obj_num, resource);
    }
}

/// A resource cache with cycle detection.
///
/// Wraps a [`HashMap`]-based cache and adds an in-progress set that tracks
/// which PDF objects are currently being parsed.  When a recursive read
/// encounters an object already in the set, [`begin_read`](ResourceCache::begin_read)
/// returns `false`, allowing callers to break the cycle gracefully.
pub struct DefaultResourceCache {
    cache: HashMap<usize, Resource>,
    in_progress: HashSet<usize>,
}

impl DefaultResourceCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            in_progress: HashSet::new(),
        }
    }
}

impl Default for DefaultResourceCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceCache for DefaultResourceCache {
    fn get(&self, obj_num: &usize) -> Option<&Resource> {
        self.cache.get(obj_num)
    }

    fn insert(&mut self, obj_num: usize, resource: Resource) {
        self.cache.insert(obj_num, resource);
    }

    fn begin_read(&mut self, obj_num: usize) -> bool {
        self.in_progress.insert(obj_num)
    }

    fn end_read(&mut self, obj_num: &usize) {
        self.in_progress.remove(obj_num);
    }

    fn is_being_read(&self, obj_num: &usize) -> bool {
        self.in_progress.contains(obj_num)
    }
}
