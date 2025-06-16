use std::collections::HashMap;

use crate::{ObjectVariant, dictionary::Dictionary, error::ObjectError};

#[derive(Default)]
pub struct ObjectCollection {
    pub map: HashMap<i32, ObjectVariant>,
}

impl ObjectCollection {
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

    pub fn get2(&self, obj: &ObjectVariant) -> Option<&ObjectVariant> {
        if let Some(num) = obj.to_object_number() {
            return self.map.get(&num);
        }

        None
    }

    pub fn get_dictionary(&self, key: i32) -> Option<&Dictionary> {
        if let Some(obj) = self.map.get(&key) {
            if let ObjectVariant::IndirectObject(inner) = obj {
                if let Some(ObjectVariant::Dictionary(dictionary)) = &inner.object {
                    return Some(dictionary.as_ref());
                }
            }
            if let ObjectVariant::Reference(object_number) = obj {
                return self.get_dictionary(*object_number);
            }
        }
        None
    }
}
