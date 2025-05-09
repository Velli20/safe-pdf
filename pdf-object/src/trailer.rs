use std::rc::Rc;

use crate::dictionary::Dictionary;

#[derive(Debug, PartialEq, Clone)]
pub struct Trailer {
    /// The dictionary object containing the trailer information.
    pub dictionary: Rc<Dictionary>,
}

impl Trailer {
    pub fn new(dictionary: Rc<Dictionary>) -> Self {
        Trailer { dictionary }
    }
}
