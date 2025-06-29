use crate::{ObjectVariant, dictionary::Dictionary, error::ObjectError, stream::StreamObject};
use std::collections::HashMap;

#[derive(Default)]
pub struct ObjectCollection {
    pub map: HashMap<i32, ObjectVariant>,
}

impl ObjectCollection {
    /// A limit to prevent infinite loops when resolving an object reference
    const MAX_DEREF: usize = 16;

    pub fn insert(&mut self, obj: ObjectVariant) -> Result<(), ObjectError> {
        let key = obj.to_object_number();
        if let Some(num) = key {
            if self.map.insert(num, obj).is_some() {
                Err(ObjectError::DuplicateKeyInObjectCollection(num))
            } else {
                Ok(())
            }
        } else {
            Err(ObjectError::TypeMismatch("Fixme", "Fixme"))
        }
    }

    pub fn get(&self, key: i32) -> Option<ObjectVariant> {
        if let Some(obj) = self.map.get(&key) {
            return Some(obj.clone());
        }
        None
    }

    pub fn resolve_object<'a>(
        &'a self,
        v: &'a ObjectVariant,
    ) -> Result<&'a ObjectVariant, ObjectError> {
        let mut current_obj = v;

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

        Ok(v)
    }

    pub fn resolve_dictionary<'a>(&'a self, v: &'a ObjectVariant) -> Option<&'a Dictionary> {
        let mut current_obj = v;

        for _ in 0..Self::MAX_DEREF {
            match current_obj {
                ObjectVariant::Dictionary(dict) => return Some(dict.as_ref()),
                ObjectVariant::Reference(object_number) => {
                    current_obj = self.map.get(object_number)?;
                }
                ObjectVariant::IndirectObject(inner) => {
                    current_obj = inner.object.as_ref()?;
                }
                ObjectVariant::Stream(s) => return Some(s.dictionary.as_ref()),
                _ => return None,
            }
        }

        None
    }

    /// Resolves a PDF object to a `StreamObject`.
    ///
    /// This function takes a reference to an `ObjectVariant` and attempts to resolve it
    /// into a `StreamObject`.
    ///
    /// To prevent infinite loops from circular references, the function will only
    /// dereference up to a fixed limit (`MAX_DEREF`).
    ///
    /// # Arguments
    ///
    /// - `v`: A reference to the `ObjectVariant` to resolve.
    ///
    /// # Returns
    ///
    /// `StreamObject` if the object can be successfully resolved to a stream
    /// or `None` if the object is not a stream, if an indirect reference cannot be
    /// resolved.
    pub fn resolve_stream<'a>(&'a self, v: &'a ObjectVariant) -> Option<&'a StreamObject> {
        let mut current_obj = v;

        for _ in 0..Self::MAX_DEREF {
            match current_obj {
                ObjectVariant::Reference(object_number) => {
                    current_obj = self.map.get(object_number)?;
                }
                ObjectVariant::Stream(s) => return Some(s.as_ref()),
                _ => return None,
            }
        }

        None
    }
}
