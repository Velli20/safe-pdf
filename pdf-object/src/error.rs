/// Represents an error that can occur while handling objects.
#[derive(Debug)]
pub enum ObjectError {
    DuplicateKeyInObjectCollection(i32),
}

impl std::fmt::Display for ObjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObjectError::DuplicateKeyInObjectCollection(key) => {
                write!(
                    f,
                    "Object with the given key {} already exists in collection",
                    key
                )
            }
        }
    }
}
