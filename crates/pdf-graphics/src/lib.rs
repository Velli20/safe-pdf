pub mod color;
pub mod transform;

use num_derive::FromPrimitive;

#[derive(Clone, Copy, PartialEq, Debug, FromPrimitive)]
pub enum LineCap {
    Butt = 0,
    Round = 1,
    Square = 2,
}

#[derive(Clone, Copy, PartialEq, Debug, FromPrimitive)]
pub enum LineJoin {
    Miter = 0,
    Round = 1,
    Bevel = 2,
}
