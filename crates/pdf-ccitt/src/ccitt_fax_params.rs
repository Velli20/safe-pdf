use pdf_object_reader::{FromPdfObject, ObjectAccess, ObjectContext, ReadResult};

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
}

/// Reads a `/DecodeParms` dictionary; missing or null keys keep the PDF-specified defaults.
impl FromPdfObject for CCITTFaxParams {
    fn from_pdf_object(context: ObjectContext<'_, impl ObjectAccess + ?Sized>) -> ReadResult<Self> {
        let mut context = context.dictionary()?;
        let defaults = Self::DEFAULT;
        Ok(Self {
            k: context.optional(b"K")?.unwrap_or(defaults.k),
            columns: context.optional(b"Columns")?.unwrap_or(defaults.columns),
            rows: context.optional(b"Rows")?.unwrap_or(defaults.rows),
            end_of_line: context
                .optional(b"EndOfLine")?
                .unwrap_or(defaults.end_of_line),
            encoded_byte_align: context
                .optional(b"EncodedByteAlign")?
                .unwrap_or(defaults.encoded_byte_align),
            end_of_block: context
                .optional(b"EndOfBlock")?
                .unwrap_or(defaults.end_of_block),
            black_is1: context.optional(b"BlackIs1")?.unwrap_or(defaults.black_is1),
            damaged_rows_before_error: context
                .optional(b"DamagedRowsBeforeError")?
                .unwrap_or(defaults.damaged_rows_before_error),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_object_reader::EmptySource;
    use pdf_object_reader::object_variant::ObjectVariant;
    use pdf_object_reader::{Dictionary, ObjectReader};

    #[test]
    fn params_from_empty_dict_uses_defaults() -> ReadResult<()> {
        let dict = Dictionary::default();
        let p = ObjectReader::new(EmptySource)
            .read::<CCITTFaxParams>(&ObjectVariant::Dictionary(dict))?;
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
    fn params_from_dict_reads_all_keys() -> ReadResult<()> {
        use pdf_object_reader::object_variant::ObjectVariant;

        let dict = Dictionary::from_entries([
            (b"K", ObjectVariant::Integer(-1)),
            (b"Columns", ObjectVariant::Integer(800)),
            (b"Rows", ObjectVariant::Integer(600)),
            (b"EndOfLine", ObjectVariant::Boolean(true)),
            (b"EncodedByteAlign", ObjectVariant::Boolean(true)),
            (b"EndOfBlock", ObjectVariant::Boolean(false)),
            (b"BlackIs1", ObjectVariant::Boolean(true)),
            (b"DamagedRowsBeforeError", ObjectVariant::Integer(2)),
        ]);

        let p = ObjectReader::new(EmptySource)
            .read::<CCITTFaxParams>(&ObjectVariant::Dictionary(dict))?;

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
