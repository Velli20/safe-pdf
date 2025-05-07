use std::{collections::HashMap, rc::Rc};

use crate::{Value, error::ObjectError, indirect_object::IndirectObjectOrReference};

#[derive(Default)]
pub struct ObjectCollection {
    pub map: HashMap<i32, Rc<IndirectObjectOrReference>>,
}

impl ObjectCollection {
    pub fn insert(&mut self, obj: Rc<IndirectObjectOrReference>) -> Result<(), ObjectError> {
        let key = obj.object_number;

        if self.map.insert(key, obj).is_some() {
            Err(ObjectError::DuplicateKeyInObjectCollection(key))
        } else {
            Ok(())
        }
    }

    pub fn get(&self, key: i32) -> Option<&Value> {
        if let Some(obj) = self.map.get(&key) {
            if let Some(inner) = &obj.object {
                if let Value::IndirectObject(s) = inner {
                    return self.get(s.object_number);
                }
                return Some(inner);
            }
        }
        None
    }
}
