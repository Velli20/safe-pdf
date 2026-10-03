//! The JPX Composition box.
//!
//! Part 2 Annex M composes a file's compositing layers onto one canvas, with
//! an instruction set that may place, crop, and animate them. A PDF image is a
//! single raster, so this decoder reconstructs one compositing layer and reads
//! the Composition Options box only to check that composing is a no-op for that
//! layer. A file that really composes several layers is refused by the caller.

use pdf_graphics::Size;

use crate::{
    Jpeg2000Error,
    box_reader::{BoxKind, Jp2Box},
    jp2::ContainerError,
};

/// Bytes in a Composition Options box body.
const OPTIONS_BYTES: usize = 10;

/// The canvas a JPX file composes its layers onto.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Composition {
    canvas: Size<u32>,
    repeat: u16,
    instruction_sets: u32,
}

impl Composition {
    /// Reads the Composition Options box and counts the instruction sets.
    ///
    /// # Errors
    ///
    /// Returns a container error when the options box is absent, repeated, or
    /// the wrong length, and a box error when a child cannot be framed.
    pub(crate) fn parse(box_view: Jp2Box<'_>) -> Result<Self, Jpeg2000Error> {
        let mut children = box_view.children();
        let options = children.require(BoxKind::CompositionOptions)?;
        let [
            height_a,
            height_b,
            height_c,
            height_d,
            width_a,
            width_b,
            width_c,
            width_d,
            repeat_hi,
            repeat_lo,
        ] = *options
            .payload()
            .first_chunk::<OPTIONS_BYTES>()
            .filter(|_| options.payload().len() == OPTIONS_BYTES)
            .ok_or_else(|| ContainerError::from(options))?;
        let canvas = Size {
            width: u32::from_be_bytes([width_a, width_b, width_c, width_d]),
            height: u32::from_be_bytes([height_a, height_b, height_c, height_d]),
        };
        if canvas.width == 0 || canvas.height == 0 {
            return Err(ContainerError::from(options).into());
        }
        let mut instruction_sets = 0u32;
        while let Some(child) = children.next_box()? {
            match child.kind() {
                BoxKind::CompositionInstruction => {
                    instruction_sets = instruction_sets.saturating_add(1);
                }
                BoxKind::CompositionOptions => {
                    return Err(ContainerError::Duplicate {
                        offset: child.offset(),
                        kind: BoxKind::CompositionOptions,
                    }
                    .into());
                }
                _ => {}
            }
        }
        Ok(Self {
            canvas,
            repeat: u16::from_be_bytes([repeat_hi, repeat_lo]),
            instruction_sets,
        })
    }

    /// Returns the extent of the composition canvas.
    pub fn canvas(&self) -> Size<u32> {
        self.canvas
    }

    /// Returns how many times the composition repeats, zero meaning once.
    pub fn repeat(&self) -> u16 {
        self.repeat
    }

    /// Returns the number of instruction sets the composition carries.
    pub fn instruction_sets(&self) -> u32 {
        self.instruction_sets
    }

    /// Returns whether composing leaves one layer of `image` size unchanged.
    ///
    /// A single layer that already fills the canvas needs no placement, so the
    /// instructions cannot move it and the composed image is the layer itself.
    pub(crate) fn is_identity_for(&self, image: Size<u32>) -> bool {
        self.canvas == image
    }
}
