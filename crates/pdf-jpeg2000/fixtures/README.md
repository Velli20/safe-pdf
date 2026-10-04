# JPEG 2000 fixtures

`manifest.json` is the Milestone 0 fixture contract. The offline Rust test
loads it without calling the unfinished decoder. Each case names its encoded
input, SIZ component geometry where meaningful, provenance, and expected
result. `source_components` holds reconstructed component planes in PGX files;
`display_pixels` holds colour-managed or PDF-rendered output. An absent
reference means the published corpus supplies only the other kind of output.
No display reference is treated as source-component data.

Download the pinned corpus into the ignored `cache/` directory, then verify
all files and the expanded PDF:

```sh
python3 crates/pdf-jpeg2000/fixtures/fetch.py fetch
python3 crates/pdf-jpeg2000/fixtures/fetch.py verify
cargo test -p pdf-jpeg2000 --test fixture_contract
```

The OpenJPEG inputs and references are pinned to `openjpeg-data` commit
`39524bd3a601d90ed8e0177559400d23945f96a9`. Its conformance files have
specific usage terms in `input/conformance/COPYRIGHT`; the fetch command also
downloads that notice. Those files are deliberately absent from Git. Their
source-component references and allowable errors come from T.803 Annex C;
the JP2 display references and peak errors come from Annex G. T.803 references
may cover only a specified crop or resolution, so future decoder comparisons
must use each reference's own dimensions.

The PDF case is pinned to PDFium commit
`d611f5db9bc161a50daa28e1a71e8cf9a1206815`. The fetch command obtains
its six image inputs, template, expected page PNG, BSD-style license notice,
and template expansion tool. The expanded PDF is checked against the manifest
hash. Its PNG is a published visual reference; PDFium describes it as manually
created, so the manifest labels those comparisons `reference_only` rather than
claiming a T.803 conformance tolerance.

The corrupt JP2 Header Box length case comes from the
[Jpylyzer test corpus](https://github.com/openpreserve/jpylyzer-test-files),
commit `0290e98bae9c5480c995954d3f14b4cf0a0395ff`. Its README records
the Bitwiser case's attribution and usage terms. The manifest also keeps
OpenJPEG's distinct invalid short-box case so future readers can distinguish
box overruns from lengths smaller than a box header.

`cargo test` never fetches data. Milestone 0 validates the fixture contract;
later milestones can add decoder comparisons as their stages become available.
