use crate::dictionary::Dictionary;

#[derive(Debug, PartialEq)]
pub struct Trailer {
    /// The dictionary object containing the trailer information.
    pub dictionary: Dictionary,
}

impl Trailer {
    pub fn new(dictionary: Dictionary) -> Self {
        Trailer { dictionary }
    }
}
