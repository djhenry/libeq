# libeq_eqg

Resource identification and raw format readers for EverQuest EQG data. Archive
extraction belongs to `libeq_pfs`; resource-provider selection, coordinate
conversion, rendering, and collision policy belong to the consuming application.

## Supported operations

- `identify`: recognize EQGZ, EQGT, EQGM, EQTZP, and EQOBG signatures. Recognition
  alone does not validate a resource body or establish support for its version.
- `zone::parse`: read binary EQGZ versions 1 and 2, including the string table,
  nullable model references, placements, variable v2 extension data, regions,
  lights, and any trailing bytes.

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
