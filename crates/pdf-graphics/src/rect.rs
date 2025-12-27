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
