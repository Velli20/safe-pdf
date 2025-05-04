#[derive(Debug, PartialEq)]
pub struct Boolean(pub bool);

impl Boolean {
    pub fn new(value: bool) -> Self {
        Boolean(value)
    }
}
