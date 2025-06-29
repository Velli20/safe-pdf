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
        let mut current_obj = v;
        // Set a limit to prevent infinite loops with circular references.
        const MAX_DEREF: usize = 16;
        for _ in 0..MAX_DEREF {
            match current_obj {
                ObjectVariant::Dictionary(dict) => return Some(dict.clone()),
                ObjectVariant::Reference(object_number) => {
                    current_obj = self.map.get(object_number)?;
                }
                ObjectVariant::IndirectObject(inner) => {
                    current_obj = inner.object.as_ref()?;
                }
                ObjectVariant::Stream(s) => return Some(s.dictionary.clone()),
                _ => return None,
            }
        }

        None // Dereference limit reached
    }
}
