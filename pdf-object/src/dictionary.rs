use std::{collections::BTreeMap, rc::Rc};

use crate::{ObjectVariant, Value, array::Array, indirect_object::IndirectObject};

#[derive(Debug, PartialEq, Clone)]
pub struct Dictionary {
    pub dictionary: BTreeMap<String, Box<Value>>,
}

impl Dictionary {
    pub fn new(dictionary: BTreeMap<String, Box<Value>>) -> Self {
        Dictionary { dictionary }
    }

    pub fn get_number(&self, key: &str) -> Option<i64> {
        self.dictionary
            .get(key)
            .and_then(|value| match value.as_ref() {
                Value::Number(number) => Some(number.integer.unwrap_or(0)),
                _ => None,
            })
    }

    pub fn get_string(&self, key: &str) -> Option<&String> {
        self.dictionary
            .get(key)
            .and_then(|value| match value.as_ref() {
                Value::Name(name) => Some(&name.0),
                _ => None,
            })
    }

    pub fn get_object(&self, key: &str) -> Option<&ObjectVariant> {
        self.dictionary
            .get(key)
            .and_then(|value| match value.as_ref() {
                Value::IndirectObject(obj) => Some(obj),
                _ => None,
            })
    }

    pub fn get_dictionary(&self, key: &str) -> Option<&Rc<Dictionary>> {
        self.dictionary
            .get(key)
            .and_then(|value| match value.as_ref() {
                Value::Dictionary(obj) => Some(obj),
                _ => None,
            })
    }

    pub fn get_array(&self, key: &str) -> Option<&Array> {
        self.dictionary
            .get(key)
            .and_then(|value| match value.as_ref() {
                Value::Array(arr) => Some(arr),
                _ => None,
            })
    }
}
