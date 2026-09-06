use bitflags::bitflags;
use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};

bitflags! {
    /// Font descriptor flags as defined in ISO 32000-1, Table 123.
    ///
    /// Bit numbering follows the PDF spec (1-based); the constants below
    /// use zero-based shifts: spec bit N → `1 << (N - 1)`.
    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct FontFlags: u32 {
        const FIXED_PITCH  = 1 << 0;  // spec bit 1
        const SERIF        = 1 << 1;  // spec bit 2
        const SYMBOLIC     = 1 << 2;  // spec bit 3
        const SCRIPT       = 1 << 3;  // spec bit 4
        // spec bit 5 is reserved
        const NON_SYMBOLIC = 1 << 5;  // spec bit 6
        const ITALIC       = 1 << 6;  // spec bit 7
        // spec bits 8–16 are reserved
        const ALL_CAP      = 1 << 16; // spec bit 17
        const SMALL_CAP    = 1 << 17; // spec bit 18
        const FORCE_BOLD   = 1 << 18; // spec bit 19
    }
}

impl FromPdfObject for FontFlags {
    /// Reads a descriptor's flag number while ignoring reserved bits.
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        Ok(Self::from_bits_truncate(u32::from_pdf_object(context)?))
    }
}
