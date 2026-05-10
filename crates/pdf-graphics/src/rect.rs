#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Rect {
    pub const UNIT_RECT: Rect = Rect {
        left: 0.0,
        top: 0.0,
        right: 1.0,
        bottom: 1.0,
    };

    /// Returns a rectangle whose edges are ordered so width and height are non-negative.
    pub fn normalized(&self) -> Self {
        Self {
            left: self.left.min(self.right),
            top: self.top.min(self.bottom),
            right: self.left.max(self.right),
            bottom: self.top.max(self.bottom),
        }
    }

    pub fn width(&self) -> f32 {
        self.right - self.left
    }

    pub fn height(&self) -> f32 {
        self.bottom - self.top
    }
}

impl From<[f32; 4]> for Rect {
    fn from(arr: [f32; 4]) -> Self {
        Rect {
            left: arr[0],
            top: arr[1],
            right: arr[2],
            bottom: arr[3],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Rect;

    #[test]
    fn normalized_orders_inverted_edges() {
        let rect = Rect {
            left: 10.0,
            top: 20.0,
            right: -5.0,
            bottom: -1.0,
        };

        assert_eq!(
            rect.normalized(),
            Rect {
                left: -5.0,
                top: -1.0,
                right: 10.0,
                bottom: 20.0,
            }
        );
    }
}
