use pdf_object::{stream::StreamObject, traits::FromStreamObject};

use crate::error::FontError;

pub struct CharacterMap {}

impl FromStreamObject for CharacterMap {
    type ResultType = Self;
    type ErrorType = FontError;

    fn from_stream_object(stream: &StreamObject) -> Result<Self::ResultType, Self::ErrorType> {
        let map = String::from_utf8_lossy(&stream.data);
        println!("map {:?}", map);
        Ok(Self {})
    }
}

mod tests {
    use super::*;

    #[test]
    fn test_character_map() {
        let input = b"/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo <<\n   /Registry (Adobe)\n   /Ordering (UCS)\n   /Supplement 0\n>> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<00><7F>\nendcodespacerange\n9 beginbfchar\n<01> <0020>\n<02> <0048>\n<03> <0065>\n<04> <006C>\n<05> <006F>\n<06> <0057>\n<07> <0072>\n<08> <0064>\n<09> <0021>\nendbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend";
    }
}
