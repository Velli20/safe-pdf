#[derive(Debug, PartialEq, Clone)]
pub struct Comment {
    text: String,
}

impl Comment {
    pub fn new(text: String) -> Self {
        Comment { text }
    }
}

impl Comment {
    pub fn text(&self) -> &str {
        &self.text
    }
}
