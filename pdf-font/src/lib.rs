pub mod characther_map;
pub mod cid_font;
pub mod error;
pub mod font;
pub mod font_descriptor;

pub enum FontFile {
    /// Type 1 PostScript font
    FontFile,
    /// TrueType font.
    FontFile2,
    /// OpenType font.
    FontFile3,
}
