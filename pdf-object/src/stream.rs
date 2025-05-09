#[derive(Debug, PartialEq, Clone)]
pub struct Stream {
    pub data: String,
}

impl Stream {
    pub fn new(data: String) -> Self {
        Stream { data }
    }
}
