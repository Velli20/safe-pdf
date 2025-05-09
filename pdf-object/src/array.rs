use crate::Value;

#[derive(Debug, PartialEq, Clone)]
pub struct Array(pub Vec<Box<Value>>);

impl Array {
    pub fn new(values: Vec<Box<Value>>) -> Self {
        Array(values)
    }
}
