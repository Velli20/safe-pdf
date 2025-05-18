use std::collections::HashMap;

use crate::{ObjectVariant, Value, dictionary::Dictionary, error::ObjectError};

#[derive(Default)]
pub struct ObjectCollection {
    pub map: HashMap<i32, ObjectVariant>,
}

impl ObjectCollection {
    pub fn insert(&mut self, obj: ObjectVariant) -> Result<(), ObjectError> {
        let key = obj.object_number();

        if self.map.insert(key, obj).is_some() {
            Err(ObjectError::DuplicateKeyInObjectCollection(key))
        } else {
            Ok(())
        }
    }

    pub fn get(&self, key: i32) -> Option<ObjectVariant> {
        if let Some(obj) = self.map.get(&key) {
            return Some(obj.clone());
        }
        None
    }

    pub fn get2(&self, obj: &ObjectVariant) -> Option<&ObjectVariant> {
        return self.map.get(&obj.object_number());
    }

    pub fn get_dictionary(&self, key: i32) -> Option<&Dictionary> {
        if let Some(obj) = self.map.get(&key) {
            if let ObjectVariant::IndirectObject(inner) = obj {
                if let Some(Value::Dictionary(dictionary)) = &inner.object {
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
