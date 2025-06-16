use std::{collections::BTreeMap, rc::Rc};

use crate::ObjectVariant;

#[derive(Debug, PartialEq, Clone)]
pub struct Dictionary {
    pub dictionary: BTreeMap<String, Box<ObjectVariant>>,
}

impl Dictionary {
    pub fn new(dictionary: BTreeMap<String, Box<ObjectVariant>>) -> Self {
        Dictionary { dictionary }
    }

    pub fn get(&self, key: &str) -> Option<&Box<ObjectVariant>> {
        self.dictionary.get(key)
    }

    pub fn get_number(&self, key: &str) -> Option<i64> {
        self.dictionary
            .get(key)
            .and_then(|value| match value.as_ref() {
                ObjectVariant::Number(number) => Some(number.integer.unwrap_or(0)),
                _ => None,
            })
    }

    pub fn get_string(&self, key: &str) -> Option<&String> {
        self.dictionary
            .get(key)
            .and_then(|value| match value.as_ref() {
                ObjectVariant::Name(name) => Some(name),
                _ => None,
            })
    }

    pub fn get_object_reference(&self, key: &str) -> Option<i32> {
        self.dictionary
            .get(key)
            .and_then(|value| match value.as_ref() {
                ObjectVariant::Reference(obj_num) => Some(*obj_num),
                _ => None,
            })
    }

    pub fn get_dictionary(&self, key: &str) -> Option<&Rc<Dictionary>> {
        self.dictionary
            .get(key)
            .and_then(|value| match value.as_ref() {
                ObjectVariant::Dictionary(obj) => Some(obj),
                _ => None,
            })
    }

    pub fn get_array(&self, key: &str) -> Option<&Vec<ObjectVariant>> {
        self.dictionary
            .get(key)
            .and_then(|value| match value.as_ref() {
                ObjectVariant::Array(arr) => Some(arr),
                _ => None,
            })
    }
}
