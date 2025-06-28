use std::{collections::HashMap, rc::Rc};

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

    pub fn get_dictionary(&self, v: &ObjectVariant) -> Option<Rc<Dictionary>> {
        match v {
            ObjectVariant::Dictionary(dict) => Some(dict.clone()),
            ObjectVariant::Reference(object_number) => {
                if let Some(obj) = self.map.get(object_number) {
                    if let ObjectVariant::IndirectObject(inner) = obj {
                        if let Some(ObjectVariant::Dictionary(dictionary)) = &inner.object {
                            return Some(dictionary.clone());
                        }
                    }
                    if let ObjectVariant::Reference(_) = obj {
                        return self.get_dictionary(obj);
                    }
                }
                None
            }
            _ => None,
        }
    }
}
