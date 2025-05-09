#[derive(Debug, PartialEq, Clone)]
pub struct Number {
    pub real: Option<f64>,
    pub integer: Option<i64>,
}

impl From<i64> for Number {
    fn from(value: i64) -> Self {
        Number {
            real: None,
            integer: Some(value),
        }
    }
}
impl From<f64> for Number {
    fn from(value: f64) -> Self {
        Number {
            real: Some(value),
            integer: None,
        }
    }
}
impl Number {
    pub fn new(value: impl Into<Number>) -> Self {
        value.into()
    }
}
