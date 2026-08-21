use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::Parser;
use pdf_cmap::cmap::{parser::CMapParser, token::CMapToken};

#[derive(Parser)]
#[command(name = "generate-cmaps")]
#[command(about = "Generate compile-time predefined CMap tables from Adobe CMap resources")]
struct Cli {
    /// Directory containing an Adobe cmap-resources checkout
    #[arg(long)]
    source_dir: PathBuf,

    /// Output Rust file
    #[arg(long, default_value = "crates/pdf-cmap/src/predefined/generated.rs")]
    output: PathBuf,
}

#[derive(Debug)]
struct ParsedCMap {
    name: String,
    use_cmap: Option<String>,
    writing_mode: u8,
    code_space_ranges: Vec<ParsedCodeSpaceRange>,
    mappings: BTreeMap<u32, u16>,
}

#[derive(Debug, Clone, Copy)]
struct ParsedCodeSpaceRange {
    start: u32,
    end: u32,
    len: u8,
}

#[derive(Debug, Clone, Copy)]
struct ParsedCidRange {
    start: u32,
    end: u32,
    cid_start: u16,
}

type ExplicitCidChars = Vec<(u32, u16)>;

fn main() -> Result<()> {
    let cli = Cli::parse();

    generate_cmaps(&cli.source_dir, &cli.output)
}

/// Generate Rust source containing compile-time predefined CMap arrays.
fn generate_cmaps(source_dir: &Path, output: &Path) -> Result<()> {
    let root = project_root()?;
    let source_dir = absolutize_from_root(&root, source_dir);
    let output = absolutize_from_root(&root, output);

    let mut files = Vec::new();
    collect_files(&source_dir, &mut files)?;

    let mut cmaps = Vec::new();
    for file in files {
        let data = fs::read(&file)
            .with_context(|| format!("Failed to read CMap source {}", file.display()))?;
        if !data
            .windows("/CMapName".len())
            .any(|window| window == b"/CMapName")
        {
            continue;
        }
        let cmap = parse_cmap_source(&data)
            .with_context(|| format!("Failed to parse CMap source {}", file.display()))?;
        if !cmap.name.is_empty() {
            cmaps.push(cmap);
        }
    }

    cmaps.sort_by(|left, right| left.name.cmp(&right.name));
    cmaps.dedup_by(|left, right| left.name == right.name);

    let rust = render_generated_cmaps(&cmaps)?;
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create output directory {}", parent.display()))?;
    }
    fs::write(&output, rust)
        .with_context(|| format!("Failed to write generated CMaps to {}", output.display()))?;

    println!("Generated {} CMaps at {}", cmaps.len(), output.display());
    Ok(())
}

/// Returns the workspace root directory.
fn project_root() -> Result<PathBuf> {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").context("CARGO_MANIFEST_DIR not set")?;
    let root = Path::new(&manifest_dir)
        .parent()
        .and_then(Path::parent)
        .context("Failed to get workspace root directory")?
        .to_path_buf();

    Ok(root)
}

/// Resolve a command-line path relative to the workspace root when needed.
fn absolutize_from_root(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

/// Recursively collect regular files under a source directory.
fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(dir).with_context(|| format!("Failed to read {}", dir.display()))? {
        let entry = entry.with_context(|| format!("Failed to read entry in {}", dir.display()))?;
        let path = entry.path();
        let metadata = entry
            .metadata()
            .with_context(|| format!("Failed to read metadata for {}", path.display()))?;
        if metadata.is_dir() {
            collect_files(&path, files)?;
        } else if metadata.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

/// Parse one Adobe CMap resource script into the generator's intermediate model.
fn parse_cmap_source(data: &[u8]) -> Result<ParsedCMap> {
    let mut parser = CMapParser::from(data);
    let mut name = String::new();
    let mut use_cmap = None;
    let mut writing_mode = 0u8;
    let mut code_space_ranges = Vec::new();
    let mut mappings = BTreeMap::new();
    let mut last_name: Option<String> = None;

    while let Some(token) = parser.next_token_lenient()? {
        match token {
            CMapToken::Name(token_name) if token_name.as_slice() == b"CMapName" => {
                name = expect_name_string(&mut parser, "missing /CMapName value")?;
                last_name = None;
            }
            CMapToken::Name(token_name) if token_name.as_slice() == b"WMode" => {
                let mode = expect_integer(&mut parser, "missing /WMode value")?;
                writing_mode = if mode == 1 { 1 } else { 0 };
                last_name = None;
            }
            CMapToken::Name(token_name) => {
                last_name = Some(String::from_utf8(token_name)?);
            }
            CMapToken::UseCMap => {
                use_cmap = last_name.take();
            }
            CMapToken::BeginCodeSpaceRange => {
                parse_generator_codespace_ranges(&mut parser, &mut code_space_ranges)?;
                last_name = None;
            }
            CMapToken::BeginCidChar => {
                parse_generator_chars(&mut parser, &mut mappings, CMapToken::EndCidChar, true)?;
                last_name = None;
            }
            CMapToken::BeginBfChar => {
                parse_generator_chars(&mut parser, &mut mappings, CMapToken::EndBfChar, false)?;
                last_name = None;
            }
            CMapToken::BeginCidRange => {
                parse_generator_ranges(&mut parser, &mut mappings, CMapToken::EndCidRange, true)?;
                last_name = None;
            }
            CMapToken::BeginBfRange => {
                parse_generator_ranges(&mut parser, &mut mappings, CMapToken::EndBfRange, false)?;
                last_name = None;
            }
            _ => {
                last_name = None;
            }
        }
    }

    Ok(ParsedCMap {
        name,
        use_cmap,
        writing_mode,
        code_space_ranges,
        mappings,
    })
}

/// Parse a `begincodespacerange` block for generated predefined CMaps.
fn parse_generator_codespace_ranges(
    parser: &mut CMapParser<'_>,
    ranges: &mut Vec<ParsedCodeSpaceRange>,
) -> Result<()> {
    loop {
        match parser.next_token_lenient()? {
            Some(CMapToken::EndCodeSpaceRange) => return Ok(()),
            Some(CMapToken::HexString(start_bytes)) => {
                let end_bytes = expect_hex(parser, "missing codespace end")?;
                let len = u8::try_from(start_bytes.len())
                    .context("codespace byte length does not fit in u8")?;
                ranges.push(ParsedCodeSpaceRange {
                    start: bytes_to_u32(&start_bytes)?,
                    end: bytes_to_u32(&end_bytes)?,
                    len,
                });
            }
            Some(_) => bail!("invalid codespace range entry"),
            None => bail!("missing endcodespacerange"),
        }
    }
}

/// Parse a `begincidchar` or `beginbfchar` block for generated predefined CMaps.
fn parse_generator_chars(
    parser: &mut CMapParser<'_>,
    mappings: &mut BTreeMap<u32, u16>,
    end_token: CMapToken,
    integer_destination: bool,
) -> Result<()> {
    loop {
        match parser.next_token_lenient()? {
            Some(token) if token == end_token => return Ok(()),
            Some(CMapToken::HexString(code_bytes)) => {
                let code = bytes_to_u32(&code_bytes)?;
                let cid = if integer_destination {
                    expect_u16_integer(parser, "missing cidchar destination")?
                } else {
                    let cid_bytes = expect_hex(parser, "missing bfchar destination")?;
                    bytes_to_u16(&cid_bytes)?
                };
                mappings.insert(code, cid);
            }
            Some(_) => bail!("invalid char mapping entry"),
            None => bail!("missing char mapping end operator"),
        }
    }
}

/// Parse a `begincidrange` or `beginbfrange` block for generated predefined CMaps.
fn parse_generator_ranges(
    parser: &mut CMapParser<'_>,
    mappings: &mut BTreeMap<u32, u16>,
    end_token: CMapToken,
    integer_destination: bool,
) -> Result<()> {
    loop {
        match parser.next_token_lenient()? {
            Some(token) if token == end_token => return Ok(()),
            Some(CMapToken::HexString(start_bytes)) => {
                let end_bytes = expect_hex(parser, "missing range end")?;
                let start = bytes_to_u32(&start_bytes)?;
                let end = bytes_to_u32(&end_bytes)?;
                if integer_destination {
                    let cid = expect_u16_integer(parser, "missing cidrange destination")?;
                    insert_sequential_mapping(mappings, start, end, cid)?;
                } else {
                    parse_bfrange_destination(parser, mappings, start, end)?;
                }
            }
            Some(_) => bail!("invalid range mapping entry"),
            None => bail!("missing range mapping end operator"),
        }
    }
}

/// Parse the destination side of one `beginbfrange` entry.
fn parse_bfrange_destination(
    parser: &mut CMapParser<'_>,
    mappings: &mut BTreeMap<u32, u16>,
    start: u32,
    end: u32,
) -> Result<()> {
    match parser.next_token_lenient()? {
        Some(CMapToken::HexString(cid_bytes)) => {
            insert_sequential_mapping(mappings, start, end, bytes_to_u16(&cid_bytes)?)?;
        }
        Some(CMapToken::LeftSquareBracket) => {
            let mut code = start;
            loop {
                match parser.next_token_lenient()? {
                    Some(CMapToken::RightSquareBracket) => return Ok(()),
                    Some(CMapToken::HexString(cid_bytes)) => {
                        if code <= end {
                            mappings.insert(code, bytes_to_u16(&cid_bytes)?);
                            code = code.saturating_add(1);
                        }
                    }
                    Some(_) => bail!("invalid bfrange array destination"),
                    None => bail!("missing bfrange array terminator"),
                }
            }
        }
        Some(_) => bail!("invalid bfrange destination"),
        None => bail!("missing bfrange destination"),
    }
    Ok(())
}

/// Expand a sequential source-code to CID range into explicit mappings.
fn insert_sequential_mapping(
    mappings: &mut BTreeMap<u32, u16>,
    start: u32,
    end: u32,
    cid_start: u16,
) -> Result<()> {
    if start > end {
        bail!("range start is greater than end");
    }

    let mut code = start;
    loop {
        let offset = code
            .checked_sub(start)
            .context("range offset underflow while generating CMap")?;
        let offset = u16::try_from(offset).context("range offset does not fit in u16")?;
        let cid = cid_start
            .checked_add(offset)
            .context("CID range destination overflow")?;
        mappings.insert(code, cid);
        if code == end {
            break;
        }
        code = code
            .checked_add(1)
            .context("source code overflow while expanding CMap range")?;
    }
    Ok(())
}

/// Read the next token as a CMap name and convert it to UTF-8.
fn expect_name_string(parser: &mut CMapParser<'_>, message: &str) -> Result<String> {
    match parser.next_token_lenient()? {
        Some(CMapToken::Name(name)) => Ok(String::from_utf8(name)?),
        Some(_) | None => bail!("{message}"),
    }
}

/// Read the next token as a decoded PDF hex string.
fn expect_hex(parser: &mut CMapParser<'_>, message: &str) -> Result<Vec<u8>> {
    match parser.next_token_lenient()? {
        Some(CMapToken::HexString(bytes)) => Ok(bytes),
        Some(_) | None => bail!("{message}"),
    }
}

/// Read the next token as an integer.
fn expect_integer(parser: &mut CMapParser<'_>, message: &str) -> Result<i64> {
    match parser.next_token_lenient()? {
        Some(CMapToken::Integer(value)) => Ok(value),
        Some(_) | None => bail!("{message}"),
    }
}

/// Read the next token as an integer that fits in a CID-sized `u16`.
fn expect_u16_integer(parser: &mut CMapParser<'_>, message: &str) -> Result<u16> {
    let value = expect_integer(parser, message)?;
    u16::try_from(value).with_context(|| format!("{message}: {value} does not fit in u16"))
}

/// Convert up to four big-endian bytes into a packed source code.
fn bytes_to_u32(bytes: &[u8]) -> Result<u32> {
    if bytes.len() > std::mem::size_of::<u32>() {
        bail!("CMap code is wider than four bytes");
    }
    Ok(bytes.iter().fold(0u32, |value, byte| {
        value.checked_shl(8).unwrap_or(0) | u32::from(*byte)
    }))
}

/// Convert one or two big-endian bytes into a CID.
fn bytes_to_u16(bytes: &[u8]) -> Result<u16> {
    match bytes {
        [byte] => Ok(u16::from(*byte)),
        [first, second] => Ok(u16::from_be_bytes([*first, *second])),
        _ => bail!("CMap CID destination is not one or two bytes"),
    }
}

/// Render parsed CMaps as deterministic Rust `const` arrays.
fn render_generated_cmaps(cmaps: &[ParsedCMap]) -> Result<String> {
    let mut out = String::new();
    push_line(
        &mut out,
        "// This is an automatically generated file. Do not edit.",
    );
    push_line(
        &mut out,
        "// Regenerate with: cargo run -p pdf-cmap --bin generate-cmaps -- --source-dir <path>",
    );
    push_line(&mut out, "//");
    push_line(
        &mut out,
        "// Source resources: https://github.com/adobe-type-tools/cmap-resources",
    );
    push_line(&mut out, "// Source license: BSD-3-Clause");
    push_line(&mut out, "#![allow(clippy::large_const_arrays)]");
    push_line(&mut out, "");
    push_line(
        &mut out,
        "use super::{GeneratedCMap, GeneratedCidChar, GeneratedCidRange, GeneratedCodeSpaceRange};",
    );
    push_line(&mut out, "");

    for cmap in cmaps {
        let ident = cmap_ident(&cmap.name);
        render_codespaces(&mut out, &ident, &cmap.code_space_ranges);
        let (ranges, chars) = compressed_mappings(&cmap.mappings)?;
        render_ranges(&mut out, &ident, &ranges);
        render_chars(&mut out, &ident, &chars);
    }

    push_line(
        &mut out,
        &format!("pub const CMAPS: [GeneratedCMap; {}] = [", cmaps.len()),
    );
    for cmap in cmaps {
        let ident = cmap_ident(&cmap.name);
        let use_cmap = cmap
            .use_cmap
            .as_ref()
            .map(|name| format!("Some(b{name:?})"))
            .unwrap_or_else(|| "None".to_string());
        push_line(
            &mut out,
            &format!(
                "    GeneratedCMap {{ name: b{:?}, use_cmap: {}, writing_mode: {}, code_space_ranges: &{}_CODESPACES, cid_ranges: &{}_RANGES, cid_chars: &{}_CHARS }},",
                cmap.name, use_cmap, cmap.writing_mode, ident, ident, ident
            ),
        );
    }
    push_line(&mut out, "];");

    Ok(out)
}

/// Render one CMap's codespace ranges.
fn render_codespaces(out: &mut String, ident: &str, ranges: &[ParsedCodeSpaceRange]) {
    push_line(
        out,
        &format!(
            "const {ident}_CODESPACES: [GeneratedCodeSpaceRange; {}] = [",
            ranges.len()
        ),
    );
    for range in ranges {
        push_line(
            out,
            &format!(
                "    GeneratedCodeSpaceRange {{ start: 0x{:X}, end: 0x{:X}, len: {} }},",
                range.start, range.end, range.len
            ),
        );
    }
    push_line(out, "];");
    push_line(out, "");
}

/// Render one CMap's sequential CID ranges.
fn render_ranges(out: &mut String, ident: &str, ranges: &[ParsedCidRange]) {
    push_line(
        out,
        &format!(
            "const {ident}_RANGES: [GeneratedCidRange; {}] = [",
            ranges.len()
        ),
    );
    for range in ranges {
        push_line(
            out,
            &format!(
                "    GeneratedCidRange {{ start: 0x{:X}, end: 0x{:X}, cid_start: {} }},",
                range.start, range.end, range.cid_start
            ),
        );
    }
    push_line(out, "];");
    push_line(out, "");
}

/// Render one CMap's explicit CID mappings.
fn render_chars(out: &mut String, ident: &str, chars: &[(u32, u16)]) {
    push_line(
        out,
        &format!(
            "const {ident}_CHARS: [GeneratedCidChar; {}] = [",
            chars.len()
        ),
    );
    for (code, cid) in chars {
        push_line(
            out,
            &format!("    GeneratedCidChar {{ code: 0x{code:X}, cid: {cid} }},"),
        );
    }
    push_line(out, "];");
    push_line(out, "");
}

/// Compress explicit mappings into sequential ranges plus singleton entries.
fn compressed_mappings(
    mappings: &BTreeMap<u32, u16>,
) -> Result<(Vec<ParsedCidRange>, ExplicitCidChars)> {
    let mut ranges = Vec::new();
    let mut chars = Vec::new();
    let mut current: Option<ParsedCidRange> = None;

    for (code, cid) in mappings {
        if let Some(mut range) = current {
            let expected_code = range.end.checked_add(1);
            let current_len = range
                .end
                .checked_sub(range.start)
                .and_then(|len| u16::try_from(len).ok())
                .and_then(|len| range.cid_start.checked_add(len));
            let expected_cid = current_len.and_then(|value| value.checked_add(1));
            if expected_code == Some(*code) && expected_cid == Some(*cid) {
                range.end = *code;
                current = Some(range);
            } else {
                flush_compressed_mapping(range, &mut ranges, &mut chars);
                current = Some(ParsedCidRange {
                    start: *code,
                    end: *code,
                    cid_start: *cid,
                });
            }
        } else {
            current = Some(ParsedCidRange {
                start: *code,
                end: *code,
                cid_start: *cid,
            });
        }
    }

    if let Some(range) = current {
        flush_compressed_mapping(range, &mut ranges, &mut chars);
    }

    Ok((ranges, chars))
}

/// Append a completed compressed mapping run to the appropriate output bucket.
fn flush_compressed_mapping(
    range: ParsedCidRange,
    ranges: &mut Vec<ParsedCidRange>,
    chars: &mut Vec<(u32, u16)>,
) {
    if range.start == range.end {
        chars.push((range.start, range.cid_start));
    } else {
        ranges.push(range);
    }
}

/// Convert a CMap name into a generated Rust identifier suffix.
fn cmap_ident(name: &str) -> String {
    let mut ident = String::from("CMAP");
    for ch in name.chars() {
        ident.push('_');
        if ch.is_ascii_alphanumeric() {
            ident.push(ch.to_ascii_uppercase());
        } else {
            ident.push('_');
        }
    }
    ident
}

/// Append one line to generated Rust source.
fn push_line(out: &mut String, line: &str) {
    out.push_str(line);
    out.push('\n');
}
