use write_fonts::{
    FontBuilder,
    tables::{head::Head, hhea::Hhea, hmtx::Hmtx},
    types::Tag,
};

use crate::error::FontError;

/// Builds a minimal OpenType font from raw CFF (Compact Font Format) bytes.
///
/// This function wraps the provided CFF data into an OpenType font structure by adding
/// essential tables (`Head`, `Hhea`, `Hmtx`) required for the font to be recognized
/// and processed by standard font tools and renderers.
///
/// # Arguments
///
/// - `cff_bytes`: A byte slice containing the raw CFF table data.
///
/// # Returns
///
/// A vector containing the built OpenType font bytes
/// on success, or a `FontError` if table addition fails.
pub(crate) fn build_cff_font(cff_bytes: &[u8]) -> Result<Vec<u8>, FontError> {
    let mut builder = FontBuilder::new();
    // Add raw CFF table.
    builder.add_raw(Tag::new(b"CFF "), cff_bytes);

    // Head table
    builder
        .add_table(&Head {
            units_per_em: 1000,
            index_to_loc_format: 0,
            ..Default::default()
        })
        .map_err(|_| FontError::FontBuildError("Failed to add Head table".to_string()))?;

    // Hhea table
    let hhea = Hhea::default();
    builder
        .add_table(&hhea)
        .map_err(|_| FontError::FontBuildError("Failed to add Hhea table".to_string()))?;

    // Hmtx table
    let hmtx = Hmtx::default();
    builder
        .add_table(&hmtx)
        .map_err(|_| FontError::FontBuildError("Failed to add Hmtx table".to_string()))?;

    Ok(builder.build())
}
