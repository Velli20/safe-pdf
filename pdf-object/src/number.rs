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

    pub fn as_f32(&self) -> Option<f32> {
        if let Some(value) = self.real {
            Some(value as f32)
        } else if let Some(value) = self.integer {
            Some(value as f32)
        } else {
            None
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        if let Some(value) = self.real {
            Some(value as i64)
        } else if let Some(value) = self.integer {
            Some(value)
        } else {
            None
        }
    }
}
