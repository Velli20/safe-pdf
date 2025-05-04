#[derive(Debug, PartialEq)]
pub struct LiteralString(pub String);

impl LiteralString {
    pub fn new(literal: String) -> Self {
        LiteralString(literal)
    }
}
