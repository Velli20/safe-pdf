use crate::{ObjectVariant, dictionary::Dictionary, error::ObjectError, stream::StreamObject};
use std::collections::HashMap;

#[derive(Default)]
pub struct ObjectCollection {
    pub map: HashMap<usize, ObjectVariant>,
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
            Err(ObjectError::ObjectMissingNumber {
                found_type: obj.name(),
            })
        }
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

                ObjectVariant::IndirectObject(inner) => {
                    if let Some(obj) = inner.object.as_ref() {
                        return Ok(obj);
                    } else {
                        return Err(ObjectError::FailedResolveDictionaryObject {
                            resolved_type: "IndirectObject",
                        });
                    }
                }

                other => return Ok(other),
            }
        }

        Ok(obj)
    }

    /// Resolves a PDF object to a `Dictionary`.
    ///
    /// This function takes a reference to an `ObjectVariant` and attempts to resolve it
    /// into a `Dictionary`.
    ///
    /// # Arguments
    ///
    /// - `obj`: A reference to the `ObjectVariant` to resolve.
    ///
    /// # Returns
    ///
    /// `Dictionary` if the object can be successfully resolved to a stream
    /// or `Err` if the object is not a stream or if an indirect reference cannot be
    /// resolved.
    pub fn resolve_dictionary<'a>(
        &'a self,
        obj: &'a ObjectVariant,
    ) -> Result<&'a Dictionary, ObjectError> {
        match self.resolve_object(obj)? {
            ObjectVariant::Dictionary(dict) => Ok(dict.as_ref()),
            ObjectVariant::Stream(s) => Ok(s.dictionary.as_ref()),
            ObjectVariant::IndirectObject(inner) => {
                if let Some(ObjectVariant::Dictionary(obj)) = inner.object.as_ref() {
                    Ok(obj.as_ref())
                } else {
                    Err(ObjectError::FailedResolveDictionaryObject {
                        resolved_type: "IndirectObject",
                    })
                }
            }
            other => Err(ObjectError::FailedResolveDictionaryObject {
                resolved_type: other.name(),
            }),
        }
    }

    /// Resolves a PDF object to a `StreamObject`.
    ///
    /// This function takes a reference to an `ObjectVariant` and attempts to resolve it
    /// into a `StreamObject`.
    ///
    /// # Arguments
    ///
    /// - `obj`: A reference to the `ObjectVariant` to resolve.
    ///
    /// # Returns
    ///
    /// `StreamObject` if the object can be successfully resolved to a stream
    /// or `Err` if the object is not a stream or if an indirect reference cannot be
    /// resolved.
    pub fn resolve_stream<'a>(
        &'a self,
        obj: &'a ObjectVariant,
    ) -> Result<&'a StreamObject, ObjectError> {
        match self.resolve_object(obj)? {
            ObjectVariant::Stream(s) => Ok(s.as_ref()),
            ObjectVariant::IndirectObject(inner) => {
                if let Some(ObjectVariant::Stream(s)) = inner.object.as_ref() {
                    Ok(s.as_ref())
                } else {
                    Err(ObjectError::FailedResolveStreamObject {
                        resolved_type: "IndirectObject",
                    })
                }
            }
            other => Err(ObjectError::FailedResolveStreamObject {
                resolved_type: other.name(),
            }),
        }
    }

    /// Resolves a PDF object to an array slice (`&[ObjectVariant]`).
    ///
    /// This function takes a reference to an `ObjectVariant` and attempts to resolve it
    /// into an array, returning a slice of its elements.
    ///
    /// - Follows indirect references via `Reference` up to `MAX_DEREF` using `resolve_object`.
    /// - If the resolved value is an `Array`, returns a slice of its contents.
    /// - If the resolved value is an `IndirectObject` that directly contains an `Array`,
    ///   returns a slice of that array.
    /// - Otherwise returns a `TypeMismatch("Array", found)` error.
    pub fn resolve_array<'a>(
        &'a self,
        obj: &'a ObjectVariant,
    ) -> Result<&'a [ObjectVariant], ObjectError> {
        match self.resolve_object(obj)? {
            ObjectVariant::Array(arr) => Ok(arr.as_slice()),
            ObjectVariant::IndirectObject(inner) => {
                if let Some(ObjectVariant::Array(arr)) = inner.object.as_ref() {
                    Ok(arr.as_slice())
                } else {
                    Err(ObjectError::TypeMismatch("Array", "IndirectObject"))
                }
            }
            other => Err(ObjectError::TypeMismatch("Array", other.name())),
        }
    }
}
