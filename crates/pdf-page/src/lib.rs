#![deny(clippy::unwrap_used)]
#![warn(clippy::expect_used)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::large_enum_variant)]

pub mod bbox;
pub mod color_space;
pub mod content_stream;
pub mod error;
pub mod external_graphics_state;
pub mod form;
pub mod function;
pub mod image;
pub mod matrix;
pub mod media_box;
pub mod page;
pub mod pages;
pub mod pattern;
pub mod resources;
pub mod shading;
pub mod xobject;
