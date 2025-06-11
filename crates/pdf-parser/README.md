# PDF Parser (`pdf-parser`)

**`pdf-parser` is a Rust crate for parsing PDF files. It processes raw PDF byte data, interprets its structure, and converts PDF objects into accessible Rust types.**

This crate is designed to be a foundational component for reading and analyzing PDF documents. It focuses on the syntactic and structural aspects of PDF files as defined by the PDF specification, providing the tools to deconstruct a PDF into its constituent parts like dictionaries, arrays, numbers, strings, streams, and structural elements like cross-reference tables and trailers.

## Overview

The primary goal of `pdf-parser` is to take a sequence of bytes representing a PDF file (or a portion of it) and produce a representation of its content, typically as `pdf_object::Value` variants or other specific Rust types. It achieves this by:

1.  Utilizing a tokenizer (from the `pdf-tokenizer` crate) to perform lexical analysis on the input byte stream, breaking it into PDF tokens.
2.  Parsing these tokens according to PDF syntax rules to identify and construct various PDF objects (numbers, strings, names, arrays, dictionaries, etc.).
3.  Handling structural components of a PDF file, such as the header, body objects, cross-reference table, and trailer.

## Modules

The crate is organized into several modules, each responsible for parsing a specific part of a PDF file or a type of PDF object:

*   `array`: Parses PDF arrays (`[...]`).
*   `boolean`: Parses PDF booleans (`true`, `false`).
*   `comment`: Parses PDF comments (`%...`).
*   `cross_reference_table`: Parses PDF cross-reference tables and sections (`xref`).
*   `dictionary`: Parses PDF dictionaries (`<<...>>`).
*   `error`: Defines `ParserError` for parsing errors.
*   `header`: Parses the PDF file header (e.g., `%PDF-1.7`).
*   `hex_string`: Parses PDF hexadecimal strings (`<...>`).
*   `indirect_object`: Parses PDF indirect objects (e.g., `1 0 obj ... endobj`).
*   `literal_string`: Parses PDF literal strings (`(...)`).
*   `name`: Parses PDF names (`/Name`).
*   `null`: Parses PDF null objects (`null`).
*   `number`: Parses PDF numbers (integers and real numbers).
*   `stream`: Parses PDF stream objects, including their dictionaries and data.
*   `trailer`: Parses PDF file trailers.

## Error Handling

Parsing operations generally return a `Result<T, ParserError>`. `ParserError` is an enum that describes various issues encountered during parsing, such as invalid syntax, unexpected tokens, or malformed data structures. This allows consuming code to handle parsing failures gracefully.
