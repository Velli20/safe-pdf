use std::collections::HashSet;

use crate::error::ObjectError;

/// Tracks object numbers that are currently being read so cyclic re-entry can be detected.
#[derive(Debug, Default)]
pub struct ObjectCycleList {
    in_progress: HashSet<usize>,
}

impl ObjectCycleList {
    /// Marks an object as in-progress.
    ///
    /// Returns an error when the same object is re-entered before the current read finishes.
    pub fn begin_read(&mut self, obj_num: usize) -> Result<(), ObjectError> {
        if !self.in_progress.insert(obj_num) {
            return Err(ObjectError::CyclicDependency { obj_num });
        }

        Ok(())
    }

    /// Marks an object as finished.
    pub fn end_read(&mut self, obj_num: usize) {
        self.in_progress.remove(&obj_num);
    }

    /// Returns true when the object is currently being read.
    pub fn is_being_read(&self, obj_num: usize) -> bool {
        self.in_progress.contains(&obj_num)
    }
}
