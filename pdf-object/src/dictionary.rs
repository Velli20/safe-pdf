use std::collections::BTreeMap;

use crate::Value;

#[derive(Debug, PartialEq)]
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
}
