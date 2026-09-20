use std::collections::BTreeSet;

use crate::{
    dictionary::Dictionary, object_error::ObjectError, object_id::ObjectId,
    object_resolver::ObjectResolver, object_variant::ObjectVariant,
};

/// Walks a dictionary and then each `/Parent` dictionary up to the root.
///
/// Only indirect `/Parent` references are followed; an inline parent ends the walk, as does the
/// first revisited node, so a malformed cycle cannot loop forever. Resolution failures surface as
/// items so callers decide whether a broken link is fatal.
pub struct ParentChain<'a> {
    next: Option<&'a Dictionary>,
    objects: &'a dyn ObjectResolver,
    visited: BTreeSet<ObjectId>,
}

impl<'a> ParentChain<'a> {
    /// Starts a walk that yields `dictionary` first, then each of its ancestors.
    pub fn new(dictionary: &'a Dictionary, objects: &'a dyn ObjectResolver) -> Self {
        Self {
            next: Some(dictionary),
            objects,
            visited: BTreeSet::new(),
        }
    }

    /// Resolves the `/Parent` of `dictionary`, or `None` when the chain ends.
    fn parent(
        &mut self,
        dictionary: &'a Dictionary,
    ) -> Result<Option<&'a Dictionary>, ObjectError> {
        let Some((parent, parent_id)) = parent_reference(dictionary) else {
            return Ok(None);
        };
        if !self.visited.insert(parent_id) {
            return Ok(None);
        }
        Ok(Some(parent.try_dictionary(self.objects)?))
    }
}

impl<'a> Iterator for ParentChain<'a> {
    type Item = Result<&'a Dictionary, ObjectError>;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next.take()?;
        match self.parent(current) {
            Ok(parent) => self.next = parent,
            Err(error) => return Some(Err(error)),
        }
        Some(Ok(current))
    }
}

/// Returns the `/Parent` entry and its object id when it is an indirect reference.
pub fn parent_reference(dictionary: &Dictionary) -> Option<(&ObjectVariant, ObjectId)> {
    match dictionary.get(b"Parent") {
        Some(parent @ ObjectVariant::Reference(parent_id)) => Some((parent, *parent_id)),
        _ => None,
    }
}
