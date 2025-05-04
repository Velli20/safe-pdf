#[derive(Debug, PartialEq)]
pub struct Stream {
    pub data: Vec<u8>,
}

impl Stream {
    pub fn new(data: Vec<u8>) -> Self {
        Stream { data }
    }
}
