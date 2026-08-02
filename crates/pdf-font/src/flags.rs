use bitflags::bitflags;
use pdf_object::{
    dictionary::Dictionary, object_lookup::ObjectLookupExt, object_resolver::ObjectResolver,
};

use crate::error::FontError;

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

impl FontFlags {
    /// Read font descriptor flags from a font dictionary.
    pub(crate) fn from_dictionary(
        dictionary: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, FontError> {
        let Some(descriptor) = dictionary.optional_dictionary("FontDescriptor", objects)? else {
            return Ok(Self::empty());
        };

        Ok(descriptor
            .get("Flags")
            .and_then(|value| value.try_number::<u32>(objects).ok())
            .map(Self::from_bits_truncate)
            .unwrap_or_default())
    }
}
