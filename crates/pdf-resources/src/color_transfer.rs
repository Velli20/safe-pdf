//! Graphics-state transfer functions (`/TR`, `/TR2`).
use crate::soft_mask::MaskTransfer;
use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ReadResult, object_variant::ObjectVariant,
};
use std::sync::Arc;

/// The red, green and blue lookup tables of a transfer function.
pub type ColorTransferTables = [[u8; 256]; 3];

/// A graphics state's transfer function, which adjusts each device color component
/// before it is painted.
pub enum ColorTransfer {
    /// `/Identity` or `/Default`, or functions that leave every component unchanged.
    Identity,
    /// The functions sampled for the red, green and blue components.
    Tables(Arc<ColorTransferTables>),
}

impl FromPdfObject for ColorTransfer {
    /// A single function applies to every component. An array lists one function per
    /// component; only the first three, for red, green and blue, apply to RGB output.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let tables = if context.object().is_array() {
            let mut array = context.array()?;
            let ComponentTransfer(red) = array.at(0)?;
            let ComponentTransfer(green) = array.at(1)?;
            let ComponentTransfer(blue) = array.at(2)?;
            [red, green, blue]
        } else {
            let ComponentTransfer(table) = context.decode()?;
            [table; 3]
        };
        let identity = ComponentTransfer::identity();
        Ok(match tables.iter().all(|table| *table == identity) {
            true => Self::Identity,
            false => Self::Tables(Arc::new(tables)),
        })
    }
}

/// One component's transfer: a function, or a name that leaves it unchanged.
struct ComponentTransfer([u8; 256]);

impl ComponentTransfer {
    /// Returns the table that maps every value to itself.
    fn identity() -> [u8; 256] {
        std::array::from_fn(|index| u8::try_from(index).unwrap_or(u8::MAX))
    }
}

impl FromPdfObject for ComponentTransfer {
    fn from_pdf_object(
        mut context: ObjectContext<'_, impl ObjectAccess + ?Sized>,
    ) -> ReadResult<Self> {
        let object = context.object();
        match object {
            ObjectVariant::String(name) if matches!(name.as_bytes(), b"Identity" | b"Default") => {
                Ok(Self(Self::identity()))
            }
            _ => Ok(Self(MaskTransfer::sample(&context.read(object)?)?)),
        }
    }
}
