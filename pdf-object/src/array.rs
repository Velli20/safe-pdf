use crate::Value;

#[derive(Debug, PartialEq)]
pub struct Array(pub Vec<Box<Value>>);

impl Array {
    pub fn new(values: Vec<Box<Value>>) -> Self {
        Array(values)
    }
}
