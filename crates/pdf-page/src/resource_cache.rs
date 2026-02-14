use crate::resource::Resource;

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
}
