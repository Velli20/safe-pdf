use pdf_object_reader::{
    dictionary::Dictionary, object_error::ObjectError, object_lookup::ObjectLookupExt,
    object_resolver::ObjectResolver,
};

/// Decode parameters for the `CCITTFaxDecode` filter (PDF spec §7.4.6, Table 11).
#[derive(Debug, Clone)]
pub struct CCITTFaxParams {
    /// Selects the encoding scheme.
    /// `K < 0` = Group 4 (T.6 MMR); `K = 0` = Group 3 1D; `K > 0` = Group 3 2D.
    /// Default: `0`.
    pub k: i32,
    /// Width of the image in pixels. Default: `1728`.
    pub columns: usize,
    /// Number of rows. `0` means decode until end-of-block / data exhaustion. Default: `0`.
    pub rows: usize,
    /// Whether EOL bit patterns appear before each row. Default: `false`.
    pub end_of_line: bool,
    /// Whether each EOL code begins on a byte boundary. Default: `false`.
    pub encoded_byte_align: bool,
    /// Whether a block terminator (EOFB / RTC) is present. Default: `true`.
    pub end_of_block: bool,
    /// If `true`, black = 1 and white = 0. Default: `false` (white = 1).
    pub black_is1: bool,
    /// Tolerated number of damaged rows before returning an error. Default: `0`.
    pub damaged_rows_before_error: u32,
}

impl Default for CCITTFaxParams {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl CCITTFaxParams {
    /// The default image width for CCITT-encoded images, used when the `/Columns` entry is missing or invalid.
    const DEFAULT_IMAGE_WIDTH: usize = 1728;
    /// The default number of rows for CCITT-encoded images, used when the `/Rows` entry is missing or invalid.
    const DEFAULT_NUMBER_OF_ROWS: usize = 0;
    /// The full set of default parameter values, used when no `/DecodeParms` entry is present.
    pub const DEFAULT: Self = Self {
        k: 0,
        columns: Self::DEFAULT_IMAGE_WIDTH,
        rows: Self::DEFAULT_NUMBER_OF_ROWS,
        end_of_line: false,
        encoded_byte_align: false,
        end_of_block: true,
        black_is1: false,
        damaged_rows_before_error: 0,
    };

    /// Build a [`CCITTFaxParams`] from a PDF `/DecodeParms` dictionary.
    ///
    /// Every key is optional; missing or invalid values fall back to the PDF
    /// specification defaults.  This impl is infallible because the spec treats
    /// all entries as optional with well-defined defaults.
    pub fn from_dictionary(
        dict: &Dictionary,
        objects: &dyn ObjectResolver,
    ) -> Result<Self, ObjectError> {
        let mut p = Self::default();

        if let Some(value) = dict.optional_number::<i32>(b"K", objects)? {
            p.k = value;
        }
        if let Some(value) = dict.optional_number::<usize>(b"Columns", objects)? {
            p.columns = value;
        }
        if let Some(value) = dict.optional_number::<usize>(b"Rows", objects)? {
            p.rows = value;
        }
        if let Some(obj) = dict.optional_boolean(b"EndOfLine", objects)? {
            p.end_of_line = obj;
        }
        if let Some(obj) = dict.optional_boolean(b"EncodedByteAlign", objects)? {
            p.encoded_byte_align = obj;
        }
        if let Some(obj) = dict.optional_boolean(b"EndOfBlock", objects)? {
            p.end_of_block = obj;
        }
        if let Some(obj) = dict.optional_boolean(b"BlackIs1", objects)? {
            p.black_is1 = obj;
        }
        if let Some(value) = dict.optional_number::<u32>(b"DamagedRowsBeforeError", objects)? {
            p.damaged_rows_before_error = value;
        }

        Ok(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_object_reader::object_resolver::PassthroughResolver;
    use pdf_object_reader::object_variant::ObjectVariant;
    use std::collections::BTreeMap;

    #[test]
    fn params_from_empty_dict_uses_defaults() -> Result<(), ObjectError> {
        use std::collections::BTreeMap;
        let dict = Dictionary::new(BTreeMap::<Vec<u8>, ObjectVariant>::new());
        let objects = PassthroughResolver;
        let p = CCITTFaxParams::from_dictionary(&dict, &objects)?;
        assert_eq!(p.k, 0);
        assert_eq!(p.columns, 1728);
        assert_eq!(p.rows, 0);
        assert!(!p.end_of_line);
        assert!(!p.encoded_byte_align);
        assert!(p.end_of_block);
        assert!(!p.black_is1);
        assert_eq!(p.damaged_rows_before_error, 0);
        Ok(())
    }

    #[test]
    fn params_from_dict_reads_all_keys() -> Result<(), ObjectError> {
        use pdf_object_reader::object_variant::ObjectVariant;

        let mut dict = Dictionary::new(BTreeMap::<Vec<u8>, ObjectVariant>::new());
        dict.dictionary
            .insert(b"K".to_vec(), ObjectVariant::Integer(-1));
        dict.dictionary
            .insert(b"Columns".to_vec(), ObjectVariant::Integer(800));
        dict.dictionary
            .insert(b"Rows".to_vec(), ObjectVariant::Integer(600));
        dict.dictionary
            .insert(b"EndOfLine".to_vec(), ObjectVariant::Boolean(true));
        dict.dictionary
            .insert(b"EncodedByteAlign".to_vec(), ObjectVariant::Boolean(true));
        dict.dictionary
            .insert(b"EndOfBlock".to_vec(), ObjectVariant::Boolean(false));
        dict.dictionary
            .insert(b"BlackIs1".to_vec(), ObjectVariant::Boolean(true));
        dict.dictionary.insert(
            b"DamagedRowsBeforeError".to_vec(),
            ObjectVariant::Integer(2),
        );

        let objects = PassthroughResolver;
        let p = CCITTFaxParams::from_dictionary(&dict, &objects)?;

        assert_eq!(p.k, -1);
        assert_eq!(p.columns, 800);
        assert_eq!(p.rows, 600);
        assert!(p.end_of_line);
        assert!(p.encoded_byte_align);
        assert!(!p.end_of_block);
        assert!(p.black_is1);
        assert_eq!(p.damaged_rows_before_error, 2);
        Ok(())
    }
}
