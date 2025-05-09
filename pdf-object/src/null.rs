#[derive(Debug, PartialEq, Clone)]
pub struct NullObject;

impl NullObject {
    pub fn new() -> Self {
        NullObject
    }
}
