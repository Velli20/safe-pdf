//! Lazy reference support for `/Resources` dictionaries.

use std::{cell::OnceCell, rc::Rc};

use crate::resources::Resources;

/// Lazily-resolved handle to a `/Resources` dictionary that is still being constructed.
///
/// The resource cache publishes this handle before recursive parsing begins so
/// later lookups can keep the cache entry alive until the owning [`Resources`]
/// value has been fully parsed.
#[derive(Clone)]
pub struct ResourcesReference {
    resources: Rc<OnceCell<Rc<Resources>>>,
}

impl ResourcesReference {
    /// Creates an unresolved handle for a `/Resources` dictionary placeholder.
    ///
    /// `object_number` identifies the PDF object whose entry is being published.
    /// The value is currently unused, but it keeps this constructor aligned with
    /// the other lazy reference helpers in the resource-loading pipeline.
    pub(crate) fn new(_object_number: usize) -> Self {
        Self {
            resources: Rc::new(OnceCell::new()),
        }
    }

    /// Publishes the parsed `/Resources` dictionary to all clones of this handle.
    pub(crate) fn resolve(&self, resources: Rc<Resources>) {
        let _ = self.resources.set(resources);
    }

    /// Returns the parsed `/Resources` dictionary once it has been published.
    pub(crate) fn resolved(&self) -> Option<&Resources> {
        self.resources.get().map(Rc::as_ref)
    }
}
