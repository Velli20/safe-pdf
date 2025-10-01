use pdf_font::font::{Font, FontSubType};
use pdf_object::{
    dictionary::Dictionary, object_collection::ObjectCollection, traits::FromDictionary,
    ObjectVariant,
};
use std::collections::BTreeMap;

#[test]
fn parse_simple_truetype_font_subtype() {
    let mut map = BTreeMap::new();
    map.insert("Subtype".into(), Box::new(ObjectVariant::Name("TrueType".into())));
    map.insert("BaseFont".into(), Box::new(ObjectVariant::Name("TestTT".into())));
    // Minimal descriptor so that parser attempts descriptor resolution optional for TrueType
    // Here we omit FontDescriptor to ensure optional path works.
    let dict = Dictionary::new(map);
    let objs = ObjectCollection::default();

    let font = Font::from_dictionary(&dict, &objs).expect("parse font");
    assert_eq!(font.subtype, FontSubType::TrueType);
    assert!(font.true_type_font.is_some());
    assert_eq!(font.true_type_font.as_ref().unwrap().base_font, "TestTT");
}
