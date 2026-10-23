# libeq_eqg

Resource identification and raw format readers for EverQuest EQG data. Archive
extraction belongs to `libeq_pfs`; resource-provider selection, coordinate
conversion, rendering, and collision policy belong to the consuming application.

## Supported operations

- `identify`: recognize EQGZ, EQGT, EQGM, EQTZP, EQOBG, and EQGP signatures. Recognition
  alone does not validate a resource body or establish support for its encoding.
- `zone::parse`: read binary EQGZ versions 1 and 2, including the string table,
  nullable model references, placements, variable v2 extension data, regions,
  lights, and any trailing bytes.
- `mesh::parse`: read EQGT terrain and EQGM model geometry at versions 1, 2,
  and 3, retaining materials, raw properties, vertex attributes, triangles,
  version-2 secondary UV data, and any trailing bytes.

- `terrain_project::parse`: borrow EQTZP project-text tokens and an explicit
  first-NUL boundary without interpreting fields or loading terrain.
- `lighting::parse` / `parse_with_limits`: read classic EQGP packed lighting
  samples without assigning color, coverage, provider, or mesh-association meanings.

The immutable `terrain_project::Project` preserves the complete `source()` and
iterates `Token { byte_offset, raw, bytes }`, including the magic token, in source
order. Leading TAB/LF/VT/FF/CR/SPACE are skipped, but only SPACE/TAB/LF separate
collected tokens. `raw` retains trailing CR/VT/FF; `bytes` trims the six whitespace
bytes. Interior CR/VT/FF, high bytes, punctuation, quotes and repeated or unknown
keys remain opaque. There is no UTF-8 requirement, grouping or token truncation.

Parsing requires an exact first `EQTZP` token at byte zero, not just the signature
prefix recognized by `identify`. Bare EOF or NUL after the magic is valid; no
newline, version or metadata fields are required. `nul_offset()` identifies the
first terminator; `trailing_data()` excludes it and distinguishes no NUL (`None`)
from a terminal NUL (`Some(b"")`). The suffix is not interpreted as inline data.
Parsing and each token pass allocate nothing, take linear time and use O(1)
extra storage. Caller-owned input, extraction and repeated passes are outside
that bound. Numeric conversion, defaults, field validation, dependency discovery,
DAT decoding and terrain/runtime admission remain consumer work.

The classic lighting layout is `EQGP`, a little-endian `u32` sample count, then
that many little-endian packed `u32` words. It has no version word. The immutable
`Lighting` view exposes `sample_count()`, borrowed `packed_bytes()`, checked
`get(index)`, and an exact-size, double-ended `packed_words()` iterator. All bits
are retained and input alignment is irrelevant. `trailing_data()` borrows any
suffix without interpreting it. Zero-count records are valid.

`lighting::Limits` defaults to 16,777,216 samples and 64 MiB of counted packed
payload. Checked count/offset arithmetic and limits precede sample access. The
limits do not bound a borrowed opaque suffix, the whole input file, archive
extraction, or total process memory. Consumers can supply different explicit
limits. Header recognition reports `Lighting { sample_count }` after all eight
header bytes, without validating the sample payload. Headerless files and other
lighting encodings are unsupported; a `.lit` filename alone establishes nothing.

Some observed `EQGP` resources contain fewer bytes than the header word would
require under this classic layout. `identify` still returns the raw
`Lighting { sample_count }` header, but `lighting::parse` rejects the short
payload with `ParseError::Truncated`. The cause is not established: identification
does not distinguish damaged classic data from another encoding sharing the
signature. Do not treat header recognition as a successful parse, pad the missing
words, or infer an alternate layout. This reader supports the classic counted-word
layout only; broader lighting support requires separate format evidence.

An optional inspection example extracts an explicitly selected archive member
and verifies its borrowed raw view. It needs locally supplied input and does not
choose a provider or interpret word semantics:

```sh
cargo run -p libeq_eqg --example lighting_probe -- ARCHIVE MEMBER
```

The zone parser checks byte bounds, declared record counts, model indices, and
terminated string references. Names remain bytes rather than assuming an encoding.
Transforms remain in source coordinates. Placement name offsets are retained as
stored; the parser does not promise they reproduce runtime actor naming in both
versions. Region/light words and placement extension data retain their original
values without assigning unverified gameplay meanings. Raw floating-point values
are preserved; consumers must validate suitability for rendering or physics.

`Zone::trailing_data` exposes any bytes after the counted records. An empty slice
means those records consumed the input; a nonempty slice is not silently discarded
or interpreted as an extension. Resource selection, model loading, and scene
assembly are outside this parser's scope.

The mesh reader validates vertex indices and terminated material/property string
references. Version 1 and 2 vertices use the 32-byte layout; version 3 uses
44 bytes with inline color and a second UV pair. Version 2 stores an additional
UV marker after triangles: marker 1 supplies secondary UVs for both kinds,
and marker 2 does so only for terrain. Missing attributes remain optional.
Triangle material values and flags are retained without assigning rendering or
collision policy. Material property types and values remain raw words; type 2
values are validated as string offsets. A material's shader offset may be the
all-ones null sentinel, reported as `None`. EQGM bone counts and trailing bytes
are exposed, but skeletal records and animation are not decoded.

## Tests

Ordinary tests construct synthetic byte fixtures and require no game installation:

```sh
cargo test -p libeq_eqg
```

An optional corpus test reads loose and archived descriptors from a supplied
installation and requires both binary versions. It fails if inputs are missing;
it does not turn a requested corpus run into a successful skip. Known RoF2 fixture
counts additionally check variable record boundaries and the region/light order.
Native assets are not distributed with this crate.

```sh
LIBEQ_TEST_RAW_DIR=/path/to/client cargo test -p libeq_eqg --test native_corpus -- --ignored --nocapture
```

A separate mesh corpus test scans all terrain members and models in six fixture
archives (`crescent`, `guildhall`, `anguish`, `row`, `shi`, and `arcstone`). It
requires both mesh kinds at all three supported versions, version-2 secondary
UV data, and opaque skeletal suffixes. Known fixture counts check record
boundaries and retention of unassigned triangle materials.

```sh
LIBEQ_TEST_RAW_DIR=/path/to/client cargo test -p libeq_eqg --test native_mesh_corpus -- --ignored --nocapture
```
