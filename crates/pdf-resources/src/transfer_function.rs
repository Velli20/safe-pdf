//! Transfer functions sampled into 8-bit lookup tables: the graphics state's `/TR`
//! and `/TR2`, and a soft mask's `/TR`.
use crate::error::PdfPagesError;
use num_traits::ToPrimitive;
use pdf_function::function::Function;
use pdf_object_reader::{
    FromPdfObject, ObjectAccess, ObjectContext, ReadResult, object_variant::ObjectVariant,
};
use std::sync::Arc;

/// The red, green and blue lookup tables of a transfer function.
pub type ColorTransferTables = [[u8; 256]; 3];

/// A graphics state's transfer function, which adjusts each device color component
/// before it is painted. `None` when every component is left unchanged.
pub(crate) struct ColorTransfer(pub Option<Arc<ColorTransferTables>>);

impl FromPdfObject for ColorTransfer {
    /// A single function applies to every component. An array lists one function per
    /// component; only the first three, for red, green and blue, apply to RGB output.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let tables = if context.object().is_array() {
            let mut array = context.array()?;
            let TransferTable(red) = array.at(0)?;
            let TransferTable(green) = array.at(1)?;
            let TransferTable(blue) = array.at(2)?;
            [red, green, blue]
        } else {
            let TransferTable(table) = context.decode()?;
            [table; 3]
        };
        let identity = TransferTable::identity();
        Ok(Self(
            (!tables.iter().all(|table| *table == identity)).then(|| Arc::new(tables)),
        ))
    }
}

/// One component's transfer: a function sampled into a lookup table, or a name that
/// leaves the component unchanged.
pub(crate) struct TransferTable([u8; 256]);

impl TransferTable {
    /// Returns the table that maps every value to itself.
    fn identity() -> [u8; 256] {
        std::array::from_fn(|index| u8::try_from(index).unwrap_or(u8::MAX))
    }

    /// Returns the lookup table, or `None` when it leaves every value unchanged.
    pub(crate) fn into_lookup(self) -> Option<Arc<[u8; 256]>> {
        (self.0 != Self::identity()).then(|| Arc::new(self.0))
    }

    /// Samples a transfer function into an 8-bit lookup table.
    fn sample(function: &Function) -> Result<Self, PdfPagesError> {
        let mut table = [0; 256];
        for (input, output) in (0_u8..=255).zip(&mut table) {
            *output = Self::sample_one(function, input)?;
        }
        Ok(Self(table))
    }

    /// Maps one 8-bit value through the transfer function.
    fn sample_one(function: &Function, input: u8) -> Result<u8, PdfPagesError> {
        match function.apply(&[f32::from(input) / 255.0])?.as_slice() {
            [value] if value.is_finite() => (value.clamp(0.0, 1.0) * 255.0).round().to_u8(),
            _ => None,
        }
        .ok_or(PdfPagesError::InvalidTransferFunction)
    }
}

impl FromPdfObject for TransferTable {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        match context.object() {
            ObjectVariant::String(name) if matches!(name.as_bytes(), b"Identity" | b"Default") => {
                Ok(Self(Self::identity()))
            }
            _ => Ok(Self::sample(&context.decode()?)?),
        }
    }
}
