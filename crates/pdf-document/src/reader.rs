use crate::catalog::Catalog;
use crate::diagnostic::PdfReadDiagnostic;
use crate::document::PdfDocument;
use crate::encrypt_objects::EncryptObjectLoader;
use crate::encryption_context::EncryptionContext;
use crate::error::PdfReaderError;
use crate::object_loader::ObjectLoader;
use crate::report::PdfReadReport;
use pdf_object_collection::object_collection::ObjectCollection;
use pdf_object_reader::{
    ObjectReader, cross_reference_table::CrossReferenceTable, trailer::Trailer,
};
use pdf_parser::parser::PdfParser;

/// Reads PDF documents from byte slices.
#[derive(Default)]
pub struct PdfReader;

impl PdfReader {
    /// Reads a PDF and returns both the document and recoverable read diagnostics.
    pub fn read_with_report(
        &self,
        input: &[u8],
        password: Option<&[u8]>,
    ) -> Result<PdfReadReport, PdfReaderError> {
        let mut parser = PdfParser::from(input);
        self.check_version(&mut parser)?;

        let CrossReferenceTable {
            entries,
            mut trailer,
        } = parser.build_xref_table()?;
        let mut diagnostics = Vec::new();
        let encryption = EncryptionContext::from_trailer(
            &mut trailer,
            &EncryptObjectLoader::new(&entries, &parser),
            password,
            &mut diagnostics,
        )?;
        let objects = ObjectLoader::new(&entries, &parser, encryption, &mut diagnostics).load()?;
        let document = self.read_document(&trailer, objects, &mut diagnostics)?;

        Ok(PdfReadReport::new(document, diagnostics))
    }

    /// Reads a PDF and discards recoverable diagnostics.
    pub fn read_from_bytes(
        &self,
        input: &[u8],
        password: Option<&[u8]>,
    ) -> Result<PdfDocument, PdfReaderError> {
        self.read_with_report(input, password)
            .map(PdfReadReport::into_document)
    }

    /// Rejects documents whose optional header declares an unsupported major version.
    fn check_version(&self, parser: &mut PdfParser) -> Result<(), PdfReaderError> {
        match parser.parse_header_in_opening_bytes()? {
            Some(version) if version.major() > 2 => Err(PdfReaderError::UnsupportedVersion(
                version.major(),
                version.minor(),
            )),
            _ => Ok(()),
        }
    }

    /// Decodes the catalog referenced by the trailer into a document.
    fn read_document(
        &self,
        trailer: &Trailer,
        objects: ObjectCollection,
        diagnostics: &mut Vec<PdfReadDiagnostic>,
    ) -> Result<PdfDocument, PdfReaderError> {
        let reader = ObjectReader::new(objects);
        let catalog = reader.read::<Catalog>(trailer.dictionary.get_or_err(b"Root")?)?;
        Ok(catalog.into_document(reader, diagnostics))
    }
}
