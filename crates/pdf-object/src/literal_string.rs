#[derive(Debug, PartialEq, Clone)]
pub struct LiteralString(pub String);

impl LiteralString {
    pub fn new(literal: String) -> Self {
        LiteralString(literal)
    }
}
