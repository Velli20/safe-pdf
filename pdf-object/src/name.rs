#[derive(Debug, PartialEq, Clone)]
pub struct Name(pub String);

impl Name {
    pub fn new(name: String) -> Self {
        Name(name)
    }
}
