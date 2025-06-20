pub mod characther_map;
pub mod cid_font;
pub mod font;
pub mod font_descriptor;
pub mod glyph_widths_map;
pub mod type3_font;

pub enum FontFile {
    /// Type 1 PostScript font
    FontFile,
    /// TrueType font.
    FontFile2,
    /// OpenType font.
    FontFile3,
}
