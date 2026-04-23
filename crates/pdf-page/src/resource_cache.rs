use std::collections::HashMap;

use crate::resource::Resource;
use pdf_object::cycle_list::ObjectCycleList;

/// A trait for managing cached PDF resources by object number.
///
/// [`ResourceCache`] provides an interface for storing and retrieving PDF
/// resources (such as fonts, images, etc.) associated with their object numbers.
/// This allows efficient reuse and lookup of resources during PDF page processing.
/// Implementors of this trait can define custom caching strategies for resource
/// management.
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

    /// Marks the object as currently being parsed.
    ///
    /// Returns `false` when the object is already in progress, indicating a cycle.
    fn begin_read(&mut self, _obj_num: usize) -> bool {
        true
    }

    /// Clears the in-progress marker for the object.
    fn end_read(&mut self, _obj_num: usize) {}

    /// Returns whether the object is currently being parsed.
    fn is_being_read(&self, _obj_num: &usize) -> bool {
        false
    }
}

/// Default resource cache used during page/resource parsing.
#[derive(Default)]
pub struct DefaultResourceCache {
    resources: HashMap<usize, Resource>,
    cycles: ObjectCycleList,
}

impl ResourceCache for DefaultResourceCache {
    fn get(&self, obj_num: &usize) -> Option<&Resource> {
        self.resources.get(obj_num)
    }

    fn insert(&mut self, obj_num: usize, resource: Resource) {
        self.resources.insert(obj_num, resource);
    }

    fn begin_read(&mut self, obj_num: usize) -> bool {
        self.cycles.begin_read(obj_num).is_ok()
    }

    fn end_read(&mut self, obj_num: usize) {
        self.cycles.end_read(obj_num);
    }

    fn is_being_read(&self, obj_num: &usize) -> bool {
        self.cycles.is_being_read(*obj_num)
    }
}

/// Runs `read` while marking `obj_num` as in progress.
///
/// Returns `Ok(None)` when the object is already being read so callers can skip
/// the cyclic branch.
pub fn read_with_cycle_guard<T, E, F>(
    cache: &mut dyn ResourceCache,
    obj_num: Option<usize>,
    read: F,
) -> Result<Option<T>, E>
where
    F: FnOnce(&mut dyn ResourceCache) -> Result<T, E>,
{
    let Some(obj_num) = obj_num else {
        return read(cache).map(Some);
    };

    if !cache.begin_read(obj_num) {
        return Ok(None);
    }

    let result = read(cache);
    cache.end_read(obj_num);
    result.map(Some)
}

impl ResourceCache for HashMap<usize, Resource> {
    fn get(&self, obj_num: &usize) -> Option<&Resource> {
        HashMap::get(self, obj_num)
    }

    fn insert(&mut self, obj_num: usize, resource: Resource) {
        HashMap::insert(self, obj_num, resource);
    }
}
