#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::as_conversions
)]

use pdf_document::{error::PdfReaderError, reader::PdfReader};

const ZERO_32: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const ZERO_48: &str = concat!(
    "000000000000000000000000000000000000000000000000",
    "000000000000000000000000000000000000000000000000"
);

struct EncryptionFixture<'a> {
    revision: i32,
    permissions: i32,
    encrypt_metadata: bool,
    owner_hash: &'a str,
    user_hash: &'a str,
    owner_encrypted_key: &'a str,
    user_encrypted_key: &'a str,
    encrypted_permissions: &'a str,
    string_filter: &'a str,
}

impl EncryptionFixture<'_> {
    fn dictionary(&self) -> String {
        format!(
            concat!(
                "<< /Filter /Standard /V 5 /R {} /Length 256 ",
                "/O <{}> /U <{}> /P {} /EncryptMetadata {} ",
                "/OE <{}> /UE <{}> /Perms <{}> ",
                "/CF << /StdCF << /CFM /AESV3 /Length 32 >> >> ",
                "/StmF /StdCF /StrF /{} >>"
            ),
            self.revision,
            self.owner_hash,
            self.user_hash,
            self.permissions,
            self.encrypt_metadata,
            self.owner_encrypted_key,
            self.user_encrypted_key,
            self.encrypted_permissions,
            self.string_filter,
        )
    }
}

fn format_xref_entry(offset: usize, generation: u16, used: bool) -> String {
    let kind = if used { 'n' } else { 'f' };
    format!("{offset:010} {generation:05} {kind} \n")
}

fn append_object(data: &mut Vec<u8>, offsets: &mut Vec<usize>, object: &[u8]) {
    offsets.push(data.len());
    data.extend_from_slice(object);
}

fn build_encrypted_pdf(fixture: &EncryptionFixture<'_>, content: Option<&[u8]>) -> Vec<u8> {
    let mut data = b"%PDF-2.0\n".to_vec();
    let mut offsets = Vec::new();

    append_object(
        &mut data,
        &mut offsets,
        b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
    );

    let encrypt_object_number = if let Some(content) = content {
        append_object(
            &mut data,
            &mut offsets,
            b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n",
        );
        append_object(
            &mut data,
            &mut offsets,
            concat!(
                "3 0 obj\n",
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] ",
                "/Resources << >> /Contents 4 0 R /Annots [5 0 R] >>\n",
                "endobj\n"
            )
            .as_bytes(),
        );

        offsets.push(data.len());
        data.extend_from_slice(
            format!("4 0 obj\n<< /Length {} >>\nstream\n", content.len()).as_bytes(),
        );
        data.extend_from_slice(content);
        data.extend_from_slice(b"\nendstream\nendobj\n");

        append_object(
            &mut data,
            &mut offsets,
            concat!(
                "5 0 obj\n",
                "<< /Type /Annot /Subtype /Text /Rect [0 0 10 10] ",
                "/Contents (identity) >>\n",
                "endobj\n"
            )
            .as_bytes(),
        );
        6
    } else {
        append_object(
            &mut data,
            &mut offsets,
            b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n",
        );
        3
    };

    offsets.push(data.len());
    data.extend_from_slice(
        format!(
            "{encrypt_object_number} 0 obj\n{}\nendobj\n",
            fixture.dictionary()
        )
        .as_bytes(),
    );

    let xref_offset = data.len();
    let object_count = offsets.len().saturating_add(1);
    data.extend_from_slice(format!("xref\n0 {object_count}\n").as_bytes());
    data.extend_from_slice(format_xref_entry(0, 65_535, false).as_bytes());
    for offset in offsets {
        data.extend_from_slice(format_xref_entry(offset, 0, true).as_bytes());
    }
    data.extend_from_slice(
        format!(
            concat!(
                "trailer\n",
                "<< /Size {} /Root 1 0 R /Encrypt {} 0 R ",
                "/ID [<646f63756d656e742d6964><646f63756d656e742d6964>] >>\n",
                "startxref\n{}\n%%EOF"
            ),
            object_count, encrypt_object_number, xref_offset
        )
        .as_bytes(),
    );

    data
}

#[test]
fn revision_6_authenticates_real_world_empty_password_dictionary() {
    let fixture = EncryptionFixture {
        revision: 6,
        permissions: -1036,
        encrypt_metadata: true,
        owner_hash: concat!(
            "1ffdfac277b56426755751c05029c2d24b4077755ca77a13e",
            "e9954fb3d05d0355095c697a02f954255b54a8660535597"
        ),
        user_hash: concat!(
            "3ceaf18c38452ccc258275458c1e863b552e70ee48e00c1cf",
            "b959cc264b945a0f546dd2b31571c100cfd45f9050c8af4"
        ),
        owner_encrypted_key: concat!(
            "6cee633cb6d1a74393e058eb620c25535415de81351febac9",
            "30e3df4b703ecc0"
        ),
        user_encrypted_key: concat!(
            "a19c8b31f295ac244fd8dcca14f8dc8affdd9f6933e6ce5",
            "633723f3fee662b21"
        ),
        encrypted_permissions: "14fedcd8c9c8396d24d9315383dff971",
        string_filter: "StdCF",
    };

    let document = PdfReader
        .read_from_bytes(&build_encrypted_pdf(&fixture, None), None)
        .expect("real-world revision 6 dictionary authenticates");

    assert_eq!(document.page_count(), 0);
}

#[test]
fn revision_5_authenticates_and_decrypts_aes_256_content() {
    let fixture = EncryptionFixture {
        revision: 5,
        permissions: -4,
        encrypt_metadata: true,
        owner_hash: ZERO_48,
        user_hash: concat!(
            "af77a8f95692e0fd7d1c72f386f80cafe320238489b51673",
            "9826e492f20e311876616c69646174656b65792d73616c74"
        ),
        owner_encrypted_key: ZERO_32,
        user_encrypted_key: concat!(
            "d274dbb8b8164ebf8b214fa9a452b5d4",
            "59ada8b98941b285b7759e3b4a6f3455"
        ),
        encrypted_permissions: "1ef05b18bbffe6e3dcd182445d3114d7",
        string_filter: "Identity",
    };
    let content = [
        0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a,
        0x5a, 0xb7, 0x6d, 0x10, 0x8a, 0x4c, 0x74, 0x14, 0xff, 0xd2, 0x21, 0x0b, 0x0b, 0xb5, 0x9a,
        0xef, 0xb9,
    ];
    let pdf = build_encrypted_pdf(&fixture, Some(&content));

    let document = PdfReader
        .read_from_bytes(&pdf, Some("pässword".as_bytes()))
        .expect("revision 5 user password authenticates");
    let page = document.get_page(0).expect("encrypted PDF contains a page");
    assert_eq!(
        page.contents
            .as_ref()
            .expect("encrypted content stream is materialized")
            .operators
            .len(),
        2
    );
    assert_eq!(
        page.annotations
            .as_ref()
            .and_then(|annotations| annotations.first())
            .and_then(|annotation| annotation.contents.as_deref()),
        Some(b"identity".as_slice())
    );

    assert!(matches!(
        PdfReader.read_from_bytes(&pdf, Some(b"wrong")),
        Err(PdfReaderError::IncorrectPassword)
    ));

    let malformed_fixture = EncryptionFixture {
        user_encrypted_key: "00000000000000000000000000000000000000000000000000000000000000",
        ..fixture
    };
    assert!(matches!(
        PdfReader.read_from_bytes(
            &build_encrypted_pdf(&malformed_fixture, Some(&content)),
            Some("pässword".as_bytes())
        ),
        Err(PdfReaderError::DecryptionSetup(message))
            if message == "invalid encrypted data: V=5 /UE entry must contain 32 bytes"
    ));
}

#[test]
fn revision_6_authenticates_owner_and_rejects_corrupt_permissions() {
    let fixture = EncryptionFixture {
        revision: 6,
        permissions: -44,
        encrypt_metadata: false,
        owner_hash: concat!(
            "9bd8ab2da3571f1a99d0a2f1ebc6d9124aedd63a4d1a19a2",
            "274c5359d05cb7db6f776e657276616c6f776e65726b6579"
        ),
        user_hash: concat!(
            "c206774951dc445f4dc7ef45968a72b81f01e05f545b3bda",
            "b46d40b909e22818757365722d76616c757365722d6b6579"
        ),
        owner_encrypted_key: concat!(
            "cacc39e9fc44a4aac93c59e03036746a",
            "a1446cbabc3b2c9ab77523ba6cbf3a40"
        ),
        user_encrypted_key: ZERO_32,
        encrypted_permissions: "d2f7619d83c6ed3d428c82f31800768f",
        string_filter: "StdCF",
    };

    let document = PdfReader
        .read_from_bytes(&build_encrypted_pdf(&fixture, None), Some(b"owner"))
        .expect("revision 6 owner password authenticates");
    assert_eq!(document.page_count(), 0);

    let corrupt_fixture = EncryptionFixture {
        encrypted_permissions: "00000000000000000000000000000000",
        ..fixture
    };
    assert!(matches!(
        PdfReader.read_from_bytes(&build_encrypted_pdf(&corrupt_fixture, None), Some(b"owner")),
        Err(PdfReaderError::DecryptionSetup(message))
            if message == "invalid encrypted data: V=5 permissions validation failed"
    ));
}
