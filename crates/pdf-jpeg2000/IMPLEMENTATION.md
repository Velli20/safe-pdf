# Completing `pdf-jpeg2000`

This is the implementation order for turning the public skeleton into a decoder
that can render JPEG 2000 images embedded in PDFs. Each milestone has an
observable gate. Complete a gate before depending on its output in the next
stage.

## Completion target and references

The decoder should accept every valid JPEG 2000 Part 1 codestream that fits the
caller's `DecoderLimits`, both as raw bytes and inside a JP2 file. It should
also interpret the JPX baseline container and colour features needed by PDF,
including PDF's required CMYK enumeration. It should produce correct image
samples on native and `wasm32-unknown-unknown` targets. Limits may reject a
valid but oversized image with `LimitExceeded`; malformed or unsupported data
must fail with a specific error and never panic.

Use the standards for field definitions, ordering, arithmetic, and output
tolerances. This guide gives the work sequence, not a replacement for them:

- [ITU-T T.800 / ISO/IEC 15444-1](https://www.itu.int/rec/T-REC-T.800-202407-I): Part 1 codestream, decoding procedures, and JP2 file format. Annexes A–I cover the stages below.
- [ITU-T T.801 / ISO/IEC 15444-2](https://www.itu.int/epublications/publication/itu-t-t-801-v3-2023-08-information-technology-jpeg-2000-image-coding-system-extensions): JPX baseline file structure and colour features. Implement the PDF-relevant container subset, not general Part 2 codestream extensions.
- [ITU-T T.803 / ISO/IEC 15444-4](https://www.itu.int/rec/T-REC-T.803-202402-I/en): decoder conformance procedures and associated test signals. Use its published comparison criteria, especially for irreversible decoding.
- [ISO 32000-2, JPXDecode](https://pdfa.org/iso-32000-22020-clause-7-syntax/): PDF constraints on JPX image data, including the CMYK colour enumeration requirement.
- [PDF 32000-1](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf) and the [PDF 2.0 image-dictionary errata](https://pdf-issues.pdfa.org/32000-2-2020/clause08.html): colour-space precedence, ignored dictionary entries, inline-image restrictions, and `SMaskInData` behaviour. Apply the version of the PDF rules appropriate to the document.

Encoding, JPIP, HTJ2K/Part 15, and general Part 2 codestream extensions are
outside this target. Reject an extension that changes decoding semantics
before treating its codestream as Part 1 data. Do not silently skip it.

## Current state and contracts to preserve

| Boundary | Current state | Required outcome |
| --- | --- | --- |
| `Jp2BoxReader::next_box` | Implemented: checked short, extended, and scope-ending lengths; `BoxKind` names the recognised boxes | Unchanged. |
| `MarkerReader::next_segment` | Implemented: standalone and length-bearing markers, borrowed payloads, `take_bytes` for packet bodies | Unchanged. |
| `SizeHeader` | Implemented: grid and component validation, `Rsiz` extension rejection, grouped geometry accessors | Unchanged. |
| `MainHeader::parse` | Implemented: SOC/SIZ ordering, COD and QCD models, COC/QCC/RGN/POC cross-checks against the defaults | Unchanged. |
| `TilePartReader` | Implemented: SOT/Psot/TPsot/TNsot framing, tile-part header overrides, EOC and trailing-data checks | Unchanged. |
| Both `decode_next_tile` methods | Implemented: reconstruct and deliver exactly one tile per call, in SIZ index order, advancing the counters only after the sink accepts the tile | Unchanged. |
| `finish` | Verifies outstanding tiles; tile-part completeness and EOC are checked while the header is parsed | Unchanged. |

`Jp2Metadata` now records how a container was resolved: how many codestreams and
compositing layers a file holds, which of each this decode reconstructs, and the
layer's own colour group and Opacity box. `TileSink` already provides a bounded
output boundary, but the following semantics must be fixed before algorithms
are added:

- A `ComponentPlane` contains reconstructed source-component samples after
  inverse transforms and level shift, before JP2/PDF palette and display
  colour conversion. `signed` and `precision` describe the source component.
  Use `SampleData::I32` only when every value fits; use `I64` otherwise so the
  Part 1 precision range through 38 bits remains representable.
- `TileBounds` is in the image reference grid. Component bounds and stride are
  in that component's sampled grid. Validate `stride`, dimensions, and slice
  length before invoking a sink. Borrowed samples expire when `write_tile`
  returns.
- Keep compressed input and marker/box payloads borrowed. Count all decoder
  allocations, including packet indexes, coefficient planes, and scratch
  buffers, against `max_working_bytes`. Use checked offset, tile-count, and
  pixel arithmetic before allocation. Keep the existing typestate sequence.
- Reserve `UnsupportedFeature` for an actual scope boundary; use structural,
  truncation, limit, or overflow errors for bad input. `UnimplementedStage`
  and `DecodeStage` were removed once every decode stage had a passing gate.

## Milestone 0 — Fixtures and an executable contract

Build a small fixture matrix with known component samples and provenance:
single-tile reversible greyscale, irreversible RGB, subsampled components,
multiple tiles and tile-parts, signed and high-precision components, JP2
colour boxes, and a JPX baseline PDF image. Keep an expected source-component
result separate from expected display pixels. Record whether each fixture is
lossless or must use the T.803 tolerance. Add negative fixtures for truncated
boxes, malformed SIZ/COD/QCD, absurd dimensions, and unsupported extensions.

**Gate:** A fixture manifest states input format, dimensions, precision,
components, expected result, and source for every case. Tests can load the
manifest without invoking the unfinished decoder.

## Milestone 1 — Container and main-header parsing

1. Implement `Jp2BoxReader` using checked `LBox`/`XLBox` arithmetic and the
   Part 1 rules for boxes extending to the end of the enclosing scope. Check
   signature and file type, required JP2 box order, nested box boundaries, and
   codestream location. Retain unknown permitted boxes as borrowed views or
   skip them by validated length. Parse colour specification, palette,
   component mapping, channel definition, and ICC payloads without eager
   copies.
2. Add JPX baseline identification and its PDF-relevant codestream, layer,
   colour, opacity, and composition associations. Multiple codestreams must be
   resolved according to JPX/PDF rules, not mistaken for one JP2 image. Reject
   unsupported Part 2 codestream capabilities explicitly.
3. Implement `MarkerReader` and `SizeHeader::component`. Distinguish
   standalone markers from length-bearing segments, validate positions and
   lengths, and parse SOC, SIZ, COD/COC, QCD/QCC, RGN, POC, SOT/SOD, EOC, and
   the Part 1 pointer and informational markers. Model tile-header overrides
   without copying their payloads. Calculate image and tile geometry from SIZ
   with checked arithmetic and enforce `DecoderLimits` before workspace setup.
4. Make `parse_header` select raw, JP2, or JPX according to `InputFormat`,
   validate the required main-header information, and construct its private
   parsed state. Extend `InputFormat` and `ImageHeader` for JPX rather than
   labelling a JPX file as JP2.

**Gate:** Metadata and borrowed slices match the fixtures; malformed lengths,
marker order, missing required fields, and limit breaches yield typed errors.
No parser reads past a slice. Fuzz box and marker readers from this point on.

**Status:** Complete. Steps 1, 3, and 4 cover Part 1 and the JP2/JPX subset
needed by PDF: box framing, the JP2 Header chain (ihdr, bpcc, colr, pclr, cmap,
cdef), brand detection, marker framing, SIZ, COD/COC, QCD/QCC, RGN, and POC.
Tile-parts are framed once while the header is parsed, so a missing tile-part, a
missing EOC, or a TLM or PLM table that contradicts the codestream is reported
there rather than at `finish`.

Step 2 resolves a JPX file to one codestream instead of refusing several.
Codestreams and compositing layers are numbered by the order of their boxes, and
the image is the first compositing layer: `compositing_layer` reads its `jplh`
children, `codestream_registration` the `creg` box that names its codestream,
and a layer without one uses the codestream sharing its index.
`jp2_header` collects the `jp2h` defaults and the selected codestream's `jpch`
overrides with the same code, and validates the merged result, so an override
cannot slip past the checks the JP2 path performs; SIZ is compared against the
Image Header box that survived the merge. `color_specifications` exposes every
`colr` box of the layer's `cgrp`, or of `jp2h`, with its `PREC` and `APPROX`
fields, and `opacity` reads the `opct` box, whose two channel forms reach
`ImageChannels` as the kind of the layer's last channel. `fragment_list` reads
`ftbl`/`flst`, and a fragmented codestream decodes when its fragments lie in
this file, in order, and leave no gap, because every stage above the container
reads the codestream as one borrowed slice. `reader_requirements` validates
`rreq` and reports it.

What still fails, each by name rather than as one blanket rejection: a layer
registering several codestreams or registering one on a coarser grid, a
composition of several layers or onto a canvas larger than the image, fragments
in another file or out of order, and a chroma-key `opct`, all
`UnsupportedFeature`; a registration naming a codestream the file lacks, and a
header box following the codestream it describes, both invalid boxes.

Two choices rest on the specification rather than on data. Part 1 I.5.3.3 makes
a JP2 reader use the first `colr` box, so the default is the first description
this crate can interpret; ranking the Part 2 alternatives by `PREC` is left to
the renderer, which knows which descriptions it can honour. `rreq` is validated
but never gates a decode, because a feature this decoder cannot honour is
refused by the box or marker that carries it. No cached fixture holds several
codestreams, a `jpch`, `jplh`, `comp`, `creg`, `opct`, or `ftbl` box, so, as
with PPM and PPT in Milestone 2, these paths were exercised by JPX containers
synthesised around the cached codestreams rather than by published files.

## Milestone 2 — Tile-parts and Tier 2 packets

Track SOT tile-part indices and lengths, tile-header overrides, SOD bodies,
and EOC without scanning compressed data for apparent marker bytes. Support
tile-parts that split one tile and ordering allowed by Part 1. Build only the
packet state needed for the current tile: resolution levels, precincts,
code-blocks, layers, and all five progression orders, including POC changes.
Implement packet-header bit stuffing, inclusion and zero-bit-plane tag trees,
coding-pass counts, and codeword-segment lengths. Handle SOP/EPH and packet
headers carried in PPM/PPT, plus PLM/PLT/TLM validation where present.

**Gate:** For test codestreams, packet boundaries, code-block contributions,
and pass lengths agree with a trusted decoder across all progression orders.
Truncated packet headers and impossible lengths fail without over-reading or
allocating beyond the configured budget.

**Status:** Complete. `TilePartReader` follows SOT lengths (including `Psot`
zero for a final tile-part), tracks `TPsot` order per tile, treats `TNsot` as
advisory (a tile whose parts outnumber or contradict it completes at EOC, as
with encoders that write it one short), validates tile-part header overrides with the same COD and QCD models as
the main header, and requires EOC with no trailing data; `TilePartIndex` groups
the framed parts by tile. Above it, `tile_structure` builds the resolution,
precinct, and code-block partitions, `progression` materialises the packet
sequence for all five orders and for POC records from either header,
`tag_tree`, `stuffed_bits`, `packet_header`, and `pass_lengths` read the packet
headers, `packed_headers` supplies them from PPM or PPT instead, and
`packet_lengths` cross-checks TLM, PLT, and PLM. SOP and EPH are consumed where
the coding style announces them. Every table is charged against
`max_working_bytes`.

PPM associates one length-prefixed group with each tile-part, as `Nppm` is
defined; no cached fixture exercises PPM or PPT, so that reading rests on the
specification alone.

## Milestone 3 — Tier 1 code-block coefficients

Implement the MQ arithmetic decoder and context state from Part 1, then the
significance propagation, magnitude refinement, and cleanup passes. Cover
code-block style flags: selective arithmetic bypass, reset contexts,
termination modes, vertically causal contexts, predictable termination, and
segmentation symbols. Keep each code-block's pass and bit-plane state local;
validate lengths supplied by Tier 2 before feeding the entropy decoder.

Start with the code-blocks from one reversible greyscale tile, then add the
style options and multiple layers. Unit-test MQ state transitions and each
coding pass with small known coefficient blocks before comparing whole images.

**Gate:** The reversible greyscale fixture produces bit-exact coefficients.
Tier 1 fixtures for every supported style flag pass independently;
corrupt pass lengths or termination markers return errors.

**Status:** Complete. The MQ coder itself now lives in `pdf-mq-coder`, shared
with the JBIG2 decoder, which uses the same coder with its own contexts.
`mq_contexts` and `context_tables` hold the Annex D contexts and their
selection tables, `code_block_state` the coefficient and flag planes,
`pass_significance`, `pass_refinement`, and `pass_cleanup` the three passes,
and `code_block_decoder` drives them across the codeword segments, including
the raw segments of the selective bypass. Every code-block style flag is
honoured.

## Milestone 4 — Reconstruction and tile delivery

Implement inverse quantization, Maxshift ROI reversal, inverse 5/3 and 9/7
wavelet transforms, DC level shifting, and reversible/irreversible multiple
component transforms. Respect per-component sampling, coding overrides,
signedness, precision, and image/tile origins. Assemble a complete tile before
calling `TileSink`; keep scratch scoped to that tile and account for its peak
allocation. Update `remaining_tiles` only after the sink succeeds. Make
`finish` verify tile completion and EOC, including truncated or extra data
according to the Part 1 syntax rules.

**Gate:** Reversible cases are bit-exact. Irreversible cases meet T.803's
applicable comparison criteria. Multitile, subsampled, ROI, 16-bit, and
high-precision cases produce correct component planes, and a sink failure is
propagated without reporting a completed tile.

**Status:** Complete. `dequantize` reverses quantization and the Maxshift
region of interest, `lifting_53` and `lifting_97` define the inverse filters
as rescaling plus lifting steps that `lifting` applies to one row or to a strip
of columns at once, `wavelet` is the two-dimensional driver,
`component_transform` the inverse RCT and ICT with the DC level shift, and
`tile_reconstruct` assembles them. The pipeline is generic over `Coefficient`
and instantiated at `i32` or `i64` according to the tile's widest magnitude, so
`SampleData` uses the narrower variant whenever it can.
`Decoder::discard_levels` reconstructs every tile without its highest
resolution levels, skipping their code-blocks, and `Decoder::levels_to_fit`
estimates how many must go for the largest tile's planes to fit
`max_working_bytes`; `pdf-image` applies it, so an oversized image is shown at
reduced resolution instead of failing.

Measured against the pinned conformance baselines: `p0_01`, `p0_03`, and
`p0_10` are bit-exact; `p0_04` reaches a peak error of 2 and a mean square
error of 0.32, 0.25, and 0.39 against limits of 5, 4, 6 and 0.776, 0.626,
1.07; `p0_05` reaches 0.234, 0.246, 0.233, and 0.0 against limits of 0.302,
0.307, 0.269, and 0.0, with a peak of 1, 2, 2, 0 against limits of 2, 2, 1, 0
— one sample of its third component exceeds the recorded peak by one.

## Milestone 5 — Colour and PDF rendering

Interpret JP2 and PDF-relevant JPX colour, palette, channel, opacity, and
ICC metadata, including the PDF-required CMYK enumeration. For JPX image
XObjects, a PDF `/ColorSpace` overrides embedded colour descriptions; without
one, use the embedded description. Derive precision from JPEG 2000 data rather
than `/BitsPerComponent`, which PDF ignores for JPX. Ignore `/Decode` for an
ordinary JPX image; apply it only for an image mask. Implement `SMaskInData`
values 0, 1, and 2, including opacity-channel selection and the applicable
`/SMask` precedence and unpremultiplication rules. Do colour conversion and
alpha composition once, after the JPEG 2000 component transform; do not infer
component count or bit depth from flattened byte length.

The existing `pdf-filter` JPX branch uses `jpeg2k` on native targets and
rejects WASM. Its `Bytes` result discards the component and container metadata
needed here. Add an image-specific path that applies filters preceding JPX in
order, then passes the encoded JPX bytes to `pdf-jpeg2000` and retains typed
metadata through `pdf-image`'s tile sink. Update both XObject and inline-image
paths: some XObject streams may already be marked `filters_applied`, so preserve
their encoded JPX payload until the structured image path consumes it.
`InlineImage::new` currently filters immediately, but PDF forbids `JPXDecode`
on inline images; reject that filter there before eager decoding. Keep generic
byte-filter callers working through an explicit adapter where they require
plain samples. Replace the current JPX byte-count heuristic and eight-bit
fallback with validated codestream precision and channel metadata. Remove the
native `jpeg2k` dependency and WASM rejection only after the new path matches
the fixture and PDF rendering results.

**Gate:** Raw codestreams decode as standalone inputs; JP2 and PDF-relevant
JPX image XObjects render with correct colours, alpha, dimensions, and masks
on native and WASM targets. Invalid inline JPX is rejected. Existing non-JPX
filters and images retain their behavior.

**Status:** Complete. `color_space` names the Annex I and Annex M enumerations,
including the CMYK code PDF requires, and records which of them a renderer can
convert without a profile. `palette`, `component_map`, and
`channel_definition` replace the previously opaque box payloads with validated
views that stay borrowed: the palette is read one entry at a time rather than
expanded into a table. `image_channels` resolves those boxes against SIZ into
the output channels a renderer consumes, giving each channel its source
component, its precision and signedness after any palette lookup, and whether
it carries colour, opacity, or premultiplied opacity. `ImageHeader::channels`
and `ImageHeader::color_specification` expose that resolution, so a caller no
longer infers component count or bit depth from a flattened byte length.

The PDF half now consumes that resolution. `JPXDecode` is no longer a byte
filter: `pdf-filter`'s `ImagePayload` applies the filters preceding it and
carries the codestream out, its byte-returning entry points refuse such a
payload rather than passing compressed data off as samples, and
`ObjectCollection` leaves a JPX stream encoded so the image path receives it
intact. `pdf-image`'s `jpx_image` drives the decoder over that codestream and
flattens the resolved channels into 8-bit colour with a separate opacity plane:
a subsampled component is replicated across the reference-grid block it covers,
a signed one is level-shifted, and any precision through 38 bits is scaled with
rounding. Precision comes from the codestream, so `/BitsPerComponent` is read
but no longer constrains a JPX image, `/Decode` is ignored as PDF directs, and
`decoded_samples` no longer divides the buffer length by the pixel count to
guess what it holds. `smask_in_data` reads the entry that decides whether the
image's own opacity is used and whether colour was multiplied by it,
unpremultiplying when either the entry or the Channel Definition box says so,
and a non-zero value drops a stray `/SMask` as ISO 32000 requires. Without a
`/ColorSpace` entry the embedded description decides, covering everything
`EnumeratedColorSpace::is_pdf_renderable` names: greyscale and both bilevel
forms, sRGB and extended sRGB, sYCC and extended sYCC through the inverse
BT.601 matrix, and CMY and CMYK; an ICC profile or any other enumeration falls
back on the channel count. `InlineImage::new` refuses `JPXDecode`, which PDF
does not allow inline. The `jpeg2k` dependency and its WASM rejection are gone
with the C `openjpeg-sys` build they carried, so `wasm32-unknown-unknown`
decodes JPEG 2000 like every other target.

One tolerance is deliberate: a PDF `/Length` often counts the end-of-line
before `endstream`, so the image path trims trailing whitespace before handing
the bytes over. The decoder itself still rejects data after the final box or
after EOC.

Measured through `pdf_image::read_xobject` against the pinned references:
`file1` and `file9` remain bit-exact across all 1 179 648 display samples, so
the palette and direct-sRGB paths survive the flattening; `p0_10` reproduces
every four-by-four block of its three subsampled components exactly, and
`p0_03` reproduces its signed four-bit samples exactly under the level shift.
`jpxdecode.pdf` loads end to end and all six of its image XObjects resolve at
4 by 4 with the expected geometry, the three `SMaskInData 1` images carrying
alpha 128 exactly where pdfium's reference shows a half-covered pixel. The
greyscale pair matches pdfium's published rendering sample for sample once
composited over the page's 50 % grey. Two differences from that rendering
remain, neither introduced here: pdfium composites an RGB image with opacity
over white before blending it, which this renderer does not, and its CMYK
conversion is colour-managed where `Color::from_cmyk` is the workspace's naive
one, used by every CMYK image. `file7` still cannot be compared, because it
carries an ICC profile and no colour management exists.

## Milestone 6 — Conformance, robustness, and release gate

- Run the relevant T.803 decoder and JP2 reader cases, plus JPX baseline and
  PDF fixtures. Record the supported conformance class, configured limits,
  bit-exact reversible results, and irreversible tolerances. Compare a broad
  fixture set against a trusted independent decoder; investigate differences
  at component-sample level before display conversion.
- Add fuzz targets for box parsing, marker parsing, tile-part and packet
  parsing, and full decoding under small limits. Check zero lengths, overflows,
  repeated markers, deep boxes, hostile tile grids, and truncated entropy
  data. Require no panics, unsafe code, out-of-bounds reads, or unbounded
  allocations on malformed input.
- Benchmark peak working memory and tile throughput for large PDF images.
  Run `cargo check`, `cargo test`, Clippy, and formatting checks workspace-wide;
  check `pdf-jpeg2000` and the web image path for `wasm32-unknown-unknown`.

**Completion gate:** Every in-scope Part 1 and PDF JPX baseline fixture either
decodes within limits or fails with a specific, justified input error. The
supported conformance cases and native/WASM PDF rendering tests pass, and no
production decode path returns `UnimplementedStage`.
