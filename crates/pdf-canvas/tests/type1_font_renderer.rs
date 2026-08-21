#![allow(
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::unwrap_used
)]

mod common;

use std::{collections::HashMap, rc::Rc};

use common::{content_stream, replay};
use pdf_canvas::{pdf_canvas::PdfCanvas, recording_canvas::RecordingCanvas};
use pdf_document::page::PdfPage;
use pdf_font::{
    encoding::Encoding,
    font::Font,
    type1_font::{Type1Font, Type1FontProgramFormat},
};
use pdf_graphics::rect::Rect;
use pdf_resources::form::FormXObject;
use pdf_resources::{resource::Resource, resources::Resources};

const EEXEC_SEED: u16 = 55665;

fn encrypt(bytes: &[u8], seed: u16) -> Vec<u8> {
    let mut r = seed;
    let mut out = Vec::with_capacity(bytes.len());
    for &plain in bytes {
        let cipher = plain ^ u8::try_from(r >> 8).expect("shifted seed should fit in u8");
        out.push(cipher);
        r = u16::try_from(
            (u32::from(cipher) + u32::from(r))
                .wrapping_mul(52845)
                .wrapping_add(22719)
                & 0xFFFF,
        )
        .expect("masked cipher state should fit in u16");
    }
    out
}

fn minimal_classic_type1_font() -> Vec<u8> {
    let cleartext = br#"%!FontType1-1.0: DummyFont 1.0
10 dict begin
/FontName /DummyFont def
/FontType 1 def
/FontMatrix [0.001 0 0 0.001 0 0] readonly def
/FontBBox [0 0 0 0] readonly def
/Encoding StandardEncoding def
currentdict end
currentfile eexec
"#;
    let private_plain = b"/Private 1 dict dup begin\n/lenIV -1 def\n/CharStrings 1 dict dup begin\n/.notdef 1 RD \x0E ND\nend\nend\nmark currentfile closefile\n";
    let mut encrypted_private = vec![0, 0, 0, 0];
    encrypted_private.extend_from_slice(private_plain);
    let encrypted_private = encrypt(&encrypted_private, EEXEC_SEED);

    let mut bytes = cleartext.to_vec();
    bytes.extend_from_slice(&encrypted_private);
    bytes.extend_from_slice(b"0000000000000000000000000000000000000000\ncleartomark\n");
    bytes
}

#[test]
fn classic_type1_renderer_uses_pdf_widths_for_advance() {
    let font = Font::Type1(Type1Font {
        font_file: minimal_classic_type1_font().into(),
        program_format: Type1FontProgramFormat::ClassicType1,
        widths: Some(HashMap::from([(65, 500.0)])),
        encoding: Encoding::default(),
        to_unicode: None,
    });
    let resources = Resources {
        fonts: HashMap::from([(
            b"F1".to_vec(),
            Resource::Font {
                font: Rc::new(font),
                resources: None,
            },
        )]),
        ..Default::default()
    };
    let stream = content_stream(1, b"BT /F1 10 Tf (AA) Tj ET");
    let page = PdfPage::default();
    let mut recording = RecordingCanvas::new(100.0, 100.0);
    let glyphs = {
        let mut canvas = PdfCanvas::new(&mut recording, &page, None)
            .expect("canvas should build")
            .with_text_recording();
        canvas
            .render_content_stream(&stream, None, None, Some(&resources), None)
            .expect("Type 1 text should render");
        canvas.take_text_glyphs()
    };

    let observer = replay(&recording);
    assert_eq!(observer.fill_colors.len(), 2);

    assert_eq!(glyphs.len(), 2);
    let first = glyphs.first().expect("first glyph should be collected");
    let second = glyphs.get(1).expect("second glyph should be collected");
    assert_eq!(&*first.unicode, ['A'].as_slice());
    assert_eq!(first.bounds.left, 0.0);
    assert_eq!(first.bounds.right, 5.0);
    assert_eq!(second.bounds.left, 5.0);
    assert_eq!(second.bounds.right, 10.0);
}

#[test]
fn classic_type1_renderer_does_not_collect_text_by_default() {
    let font = Font::Type1(Type1Font {
        font_file: minimal_classic_type1_font().into(),
        program_format: Type1FontProgramFormat::ClassicType1,
        widths: Some(HashMap::from([(65, 500.0)])),
        encoding: Encoding::default(),
        to_unicode: None,
    });
    let resources = Resources {
        fonts: HashMap::from([(
            b"F1".to_vec(),
            Resource::Font {
                font: Rc::new(font),
                resources: None,
            },
        )]),
        ..Default::default()
    };
    let stream = content_stream(1, b"BT /F1 10 Tf (A) Tj ET");
    let page = PdfPage::default();
    let mut recording = RecordingCanvas::new(100.0, 100.0);
    let mut canvas = PdfCanvas::new(&mut recording, &page, None).expect("canvas should build");

    canvas
        .render_content_stream(&stream, None, None, Some(&resources), None)
        .expect("Type 1 text should render");

    assert!(canvas.take_text_glyphs().is_empty());
}

#[test]
fn text_recording_includes_nested_form_xobjects() {
    let font = Font::Type1(Type1Font {
        font_file: minimal_classic_type1_font().into(),
        program_format: Type1FontProgramFormat::ClassicType1,
        widths: Some(HashMap::from([(65, 500.0)])),
        encoding: Encoding::default(),
        to_unicode: None,
    });
    let form_resources = Rc::new(Resources {
        fonts: HashMap::from([(
            b"F1".to_vec(),
            Resource::Font {
                font: Rc::new(font),
                resources: None,
            },
        )]),
        ..Default::default()
    });
    let form = FormXObject {
        bbox: Rect::new(20.0, 20.0),
        matrix: None,
        resources: Some(form_resources),
        content_stream: content_stream(2, b"BT /F1 10 Tf (A) Tj ET"),
    };
    let resources = Resources {
        xobjects: HashMap::from([(b"Form".to_vec(), Resource::from(form))]),
        ..Default::default()
    };
    let stream = content_stream(1, b"/Form Do");
    let page = PdfPage::default();
    let mut recording = RecordingCanvas::new(100.0, 100.0);
    let mut canvas = PdfCanvas::new(&mut recording, &page, None)
        .expect("canvas should build")
        .with_text_recording();

    canvas
        .render_content_stream(&stream, None, None, Some(&resources), None)
        .expect("nested form text should render");

    let glyphs = canvas.take_text_glyphs();
    assert_eq!(glyphs.len(), 1);
    assert_eq!(&*glyphs.first().expect("glyph should exist").unicode, ['A']);
}
