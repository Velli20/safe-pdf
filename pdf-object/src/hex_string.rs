#[derive(Debug, PartialEq)]
pub struct HexString(pub String);

impl HexString {
    pub fn new(hex: String) -> Self {
        HexString(hex)
    }
}
