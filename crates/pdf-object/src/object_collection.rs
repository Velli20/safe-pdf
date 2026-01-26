use crate::{ObjectVariant, error::ObjectError, indirect_object::IndirectObject};
use std::collections::HashMap;

#[derive(Default)]
pub struct ObjectCollection {
    pub map: HashMap<usize, ObjectVariant>,
}

impl ObjectCollection {
    /// A limit to prevent infinite loops when resolving an object reference
    const MAX_DEREF: usize = 16;

    /// Inserts a PDF object into the collection.
    ///
    /// This method handles different object variants and stores them using their
    /// appropriate keys:
    /// - `IndirectObject`: Stored by its `object_number`, with the inner object extracted.
    /// - `Stream`: Stored by its `object_number`.
    /// - `Reference`: Stored by the referenced object number.
    /// - Other variants are ignored and not stored.
    ///
    /// # Parameters
    ///
    /// - `obj`: The [`ObjectVariant`] to insert into the collection.
    ///
    /// # Returns
    ///
    /// An error if a duplicate key is detected otherwise `Ok(())`.
    pub fn insert(&mut self, obj: ObjectVariant) -> Result<(), ObjectError> {
        match obj {
            ObjectVariant::IndirectObject(indirect) => {
                let IndirectObject {
                    object_number,
                    object,
                    ..
                } = *indirect;
                let Some(object) = object else {
                    return Ok(());
                };

                if self.map.insert(object_number, object).is_some() {
                    return Err(ObjectError::DuplicateKeyInObjectCollection(object_number));
                }
            }
            ObjectVariant::Stream(ref stream) => {
                let key = stream.object_number;
                if self.map.insert(key, obj).is_some() {
                    return Err(ObjectError::DuplicateKeyInObjectCollection(key));
                }
            }
            ObjectVariant::Reference(ref reference) => {
                if self.map.insert(*reference, obj.clone()).is_some() {
                    return Err(ObjectError::DuplicateKeyInObjectCollection(*reference));
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub fn get(&self, key: usize) -> Option<&ObjectVariant> {
        if let Some(obj) = self.map.get(&key) {
            return Some(obj);
        }
        None
    }

    pub fn resolve_object<'a>(
        &'a self,
        obj: &'a ObjectVariant,
    ) -> Result<&'a ObjectVariant, ObjectError> {
        let mut current_obj = obj;

        for _ in 0..Self::MAX_DEREF {
            match current_obj {
                ObjectVariant::Reference(object_number) => {
                    if let Some(obj) = self.map.get(object_number) {
                        current_obj = obj;
                    } else {
                        return Err(ObjectError::FailedResolveObjectReference {
                            obj_num: *object_number,
                        });
                    }
                }
                other => return Ok(other),
            }
        }

        Ok(obj)
    }
}
