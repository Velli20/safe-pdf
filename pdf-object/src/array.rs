use crate::Value;

#[derive(Debug, PartialEq, Clone)]
pub struct Array(pub Vec<Value>);

impl Array {
    pub fn new(values: Vec<Value>) -> Self {
        Array(values)
    }
}
