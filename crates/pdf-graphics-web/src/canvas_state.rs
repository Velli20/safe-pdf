//! Browser-local saved clip positions; mask content uses a separate temporary backend.
#![deny(missing_docs, clippy::missing_docs_in_private_items)]
use crate::error::{WebCanvasBackendError as Error, WebResult};

/// A prepared clipping operation in logical device coordinates.
#[derive(Clone)]
pub(crate) struct Clip {
    /// Browser-native clipping geometry.
    pub(crate) path: web_sys::Path2d,
    /// Fill rule used to interpret the geometry.
    pub(crate) rule: web_sys::CanvasWindingRule,
}

/// Browser bookkeeping needed to copy clipping into temporary surfaces.
#[derive(Default)]
pub(crate) struct BackendState {
    /// Clip-list lengths captured by native saves.
    pub(crate) saved: Vec<usize>,
    /// Ordered intersections defining the current clip.
    pub(crate) clips: Vec<Clip>,
}

impl BackendState {
    /// Reserves storage before a native save changes browser state.
    pub(crate) fn reserve_save(&mut self) -> WebResult<()> {
        self.saved.try_reserve(1).map_err(|_| Error::ResourceLimit)
    }

    /// Records the clip boundary after a successful native save.
    pub(crate) fn push_saved(&mut self) {
        self.saved.push(self.clips.len());
    }

    /// Restores a clip boundary without crossing the backend's initial state.
    pub(crate) fn pop_saved(&mut self) -> WebResult<()> {
        let count = self.saved.pop().ok_or(Error::UnbalancedState)?;
        self.clips.truncate(count);
        Ok(())
    }

    /// Checks that all browser saves were restored at a page or temporary-surface boundary.
    pub(crate) fn ensure_balanced(&self) -> WebResult<()> {
        if self.saved.is_empty() {
            Ok(())
        } else {
            Err(Error::UnbalancedState)
        }
    }
}
