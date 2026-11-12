//! Raw packed-project DAT version 20, with explicit caller-supplied dimensions.
//!
//! Payload arrays, names and blobs are borrowed without interpreting terrain,
//! materials, transforms or gameplay roles. Other DAT envelopes are unsupported.

use std::ops::Range;

/// Cumulative logical limits, independent of allocator layout and total RSS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_tiles: usize,
    /// Layers plus all section A–D records, across every tile.
    pub max_records: usize,
    /// All names, including every NUL terminator and the header name.
    pub max_name_bytes: usize,
    /// Entire consumed prefix, including counts, names and borrowed payloads.
    pub max_parsed_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_tiles: 4096,
            max_records: 262_144,
            max_name_bytes: 8 * 1024 * 1024,
            max_parsed_bytes: 128 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("unsupported packed DAT version {version}")]
    UnsupportedVersion { version: u32 },
    #[error("unsupported quads-per-tile dimension {quads_per_tile}")]
    InvalidDimensions { quads_per_tile: u32 },
    #[error("byte or record count overflow in {context}")]
    CountOverflow { context: &'static str },
    #[error("truncated {context} at byte {offset}: need {needed} bytes, have {remaining}")]
    Truncated {
        context: &'static str,
        offset: usize,
        needed: usize,
        remaining: usize,
    },
    #[error("{resource} limit {limit} exceeded by request for {requested}")]
    LimitExceeded {
        resource: &'static str,
        limit: usize,
        requested: usize,
    },
    #[error("cannot reserve descriptors for {context}")]
    AllocationFailed { context: &'static str },
}

/// Immutable validated document; only descriptor vectors are owned.
#[derive(Debug)]
pub struct PackedV20<'a> {
    source: &'a [u8],
    header: Header<'a>,
    quads_per_tile: u32,
    tiles: Vec<TileV20<'a>>,
    consumed_len: usize,
}
impl<'a> PackedV20<'a> {
    pub fn source(&self) -> &'a [u8] {
        self.source
    }
    pub fn header(&self) -> &Header<'a> {
        &self.header
    }
    /// Supplied dimension, not inferred from metadata or file length.
    pub fn quads_per_tile(&self) -> u32 {
        self.quads_per_tile
    }
    pub fn tiles(&self) -> &[TileV20<'a>] {
        &self.tiles
    }
    pub fn consumed_len(&self) -> usize {
        self.consumed_len
    }
    /// Opaque suffix after the final counted tile; excluded from all limits.
    pub fn trailing_data(&self) -> &'a [u8] {
        &self.source[self.consumed_len..]
    }
}

#[derive(Debug)]
pub struct Header<'a> {
    /// Byte zero through the tile-count word, including the terminated name.
    pub source_range: Range<usize>,
    pub words: [u32; 3],
    pub name: Name<'a>,
    pub tile_count: u32,
}
#[derive(Debug, Clone, Copy)]
pub struct Name<'a> {
    /// Original byte offset of the name, including an empty name.
    pub byte_offset: usize,
    /// Exact source bytes, excluding the NUL terminator; no UTF-8 assumption.
    pub bytes: &'a [u8],
}
#[derive(Debug)]
pub struct TileV20<'a> {
    /// Entire counted tile, including its five section-count words.
    pub source_range: Range<usize>,
    pub coordinates: [u32; 2],
    pub identifier: u32,
    /// Three arrays of `(q + 1)^2` little-endian raw four-byte words.
    pub grid_bytes: [&'a [u8]; 3],
    /// Exactly `q^2` raw bytes.
    pub quad_bytes: &'a [u8],
    pub prefix_words: [u32; 2],
    pub layers: Vec<Layer<'a>>,
    pub section_a: Vec<RecordA<'a>>,
    pub section_b: Vec<RecordB<'a>>,
    pub section_c: Vec<RecordC<'a>>,
    pub section_d: Vec<RecordD<'a>>,
}
#[derive(Debug)]
pub struct Layer<'a> {
    /// Name and optional coverage, excluding the containing layer-count word.
    pub source_range: Range<usize>,
    pub name: Name<'a>,
    /// First layer: `None`. Every later layer: `Some`, including resolution zero.
    pub coverage: Option<Coverage<'a>>,
}
#[derive(Debug)]
pub struct Coverage<'a> {
    /// Resolution word and its square byte payload.
    pub source_range: Range<usize>,
    pub resolution: u32,
    pub bytes: &'a [u8],
}
/// Each A–D range covers its serialized record, excluding the containing count.
#[derive(Debug)]
pub struct RecordA<'a> {
    pub source_range: Range<usize>,
    pub name1: Name<'a>,
    pub name2: Name<'a>,
    pub blob: &'a [u8],
    pub byte: u8,
}
#[derive(Debug)]
pub struct RecordB<'a> {
    pub source_range: Range<usize>,
    pub name1: Name<'a>,
    pub word: u32,
    pub name2: Name<'a>,
    pub blob: &'a [u8],
}
#[derive(Debug)]
pub struct RecordC<'a> {
    pub source_range: Range<usize>,
    pub name1: Name<'a>,
    pub name2: Name<'a>,
    pub byte: u8,
    pub blob: &'a [u8],
}
#[derive(Debug)]
pub struct RecordD<'a> {
    pub source_range: Range<usize>,
    pub name: Name<'a>,
    pub blob: &'a [u8],
}

/// Parse the explicitly selected packed-project envelope at version 20.
///
/// `quads_per_tile` must be positive; it is supplied independently of the DAT
/// version and is never defaulted or inferred. Raw floating-point words, flags,
/// names, order and duplicates remain unchanged. No finite-value admission,
/// resource lookup, topology, coordinate conversion or consumer defaults apply.
///
/// Uses [`Limits::default`]. See [`parse_packed_v20_with_limits`] for the bounded
/// validation and allocation policy.
///
/// ```
/// use libeq_eqg::terrain_dat;
/// let mut bytes = Vec::new();
/// for word in [20_u32, 7, 9] { bytes.extend(word.to_le_bytes()); }
/// bytes.push(0); // Empty header name.
/// bytes.extend(0_u32.to_le_bytes()); // Zero tiles.
/// bytes.extend(b"opaque");
/// let parsed = terrain_dat::parse_packed_v20(&bytes, 2)?;
/// assert_eq!(parsed.header().words, [20, 7, 9]);
/// assert_eq!(parsed.consumed_len(), 17);
/// assert!(parsed.tiles().is_empty());
/// assert_eq!(parsed.trailing_data(), b"opaque");
/// # Ok::<(), terrain_dat::ParseError>(())
/// ```
pub fn parse_packed_v20(input: &[u8], quads_per_tile: u32) -> Result<PackedV20<'_>, ParseError> {
    parse_packed_v20_with_limits(input, quads_per_tile, &Limits::default())
}

/// Validate the complete counted stream before any descriptor reservation.
///
/// The same grammar runs twice: allocation-free structural preflight, then
/// construction using fallible reservations. Each pass has fresh cumulative
/// counters. Byte arrays are borrowed and are not expanded per grid cell.
/// Names are searched only within remaining input, name budget and prefix budget;
/// every terminator is charged. Successful indexed parsing allocates descriptors.
/// Payload borrows can outlive the result; descriptor slices borrow the result.
///
/// Limits exclude the opaque suffix, caller-owned input, extraction, allocator
/// overhead, subsequent caller iteration and total process memory. This API does
/// not auto-detect individual tile envelopes or other DAT families/versions, and
/// establishes neither terrain semantics nor runtime support.
pub fn parse_packed_v20_with_limits<'a>(
    input: &'a [u8],
    quads_per_tile: u32,
    limits: &Limits,
) -> Result<PackedV20<'a>, ParseError> {
    read(input, quads_per_tile, limits, false)?;
    read(input, quads_per_tile, limits, true)
}

fn overflow(context: &'static str) -> ParseError {
    ParseError::CountOverflow { context }
}
fn product(count: usize, stride: usize, context: &'static str) -> Result<usize, ParseError> {
    count.checked_mul(stride).ok_or_else(|| overflow(context))
}
fn limit(resource: &'static str, requested: usize, maximum: usize) -> Result<(), ParseError> {
    if requested > maximum {
        return Err(ParseError::LimitExceeded {
            resource,
            limit: maximum,
            requested,
        });
    }
    Ok(())
}
fn reserve<T>(
    values: &mut Vec<T>,
    count: usize,
    context: &'static str,
    store: bool,
) -> Result<(), ParseError> {
    if store {
        values
            .try_reserve_exact(count)
            .map_err(|_| ParseError::AllocationFailed { context })?;
    }
    Ok(())
}
struct Reader<'a, 'l> {
    source: &'a [u8],
    offset: usize,
    names: usize,
    records: usize,
    limits: &'l Limits,
}
impl<'a> Reader<'a, '_> {
    fn require(&self, needed: usize, context: &'static str) -> Result<usize, ParseError> {
        let end = self
            .offset
            .checked_add(needed)
            .ok_or_else(|| overflow(context))?;
        limit("parsed bytes", end, self.limits.max_parsed_bytes)?;
        if end > self.source.len() {
            return Err(ParseError::Truncated {
                context,
                offset: self.offset,
                needed,
                remaining: self.source.len().saturating_sub(self.offset),
            });
        }
        Ok(end)
    }
    fn take(&mut self, needed: usize, context: &'static str) -> Result<&'a [u8], ParseError> {
        let end = self.require(needed, context)?;
        let bytes = &self.source[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }
    fn word(&mut self, context: &'static str) -> Result<u32, ParseError> {
        let b = self.take(4, context)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn byte(&mut self, context: &'static str) -> Result<u8, ParseError> {
        Ok(self.take(1, context)?[0])
    }
    fn name(&mut self, context: &'static str) -> Result<Name<'a>, ParseError> {
        let remaining = &self.source[self.offset..];
        let prefix_room = self.limits.max_parsed_bytes.saturating_sub(self.offset);
        let name_room = self.limits.max_name_bytes.saturating_sub(self.names);
        let scan_len = remaining.len().min(prefix_room).min(name_room);
        if let Some(end) = remaining[..scan_len].iter().position(|&b| b == 0) {
            let count = end.checked_add(1).ok_or_else(|| overflow(context))?;
            let byte_offset = self.offset;
            self.names = self
                .names
                .checked_add(count)
                .ok_or_else(|| overflow(context))?;
            let raw = self.take(count, context)?;
            return Ok(Name {
                byte_offset,
                bytes: &raw[..end],
            });
        }
        let needed = scan_len.checked_add(1).ok_or_else(|| overflow(context))?;
        if scan_len == remaining.len() {
            return Err(ParseError::Truncated {
                context,
                offset: self.offset,
                needed,
                remaining: remaining.len(),
            });
        }
        let (resource, maximum, used) = if name_room < prefix_room {
            ("name bytes", self.limits.max_name_bytes, self.names)
        } else {
            ("parsed bytes", self.limits.max_parsed_bytes, self.offset)
        };
        Err(ParseError::LimitExceeded {
            resource,
            limit: maximum,
            requested: used.checked_add(needed).ok_or_else(|| overflow(context))?,
        })
    }
    fn count(&mut self, context: &'static str) -> Result<usize, ParseError> {
        let count = usize::try_from(self.word(context)?).map_err(|_| overflow(context))?;
        self.records = self
            .records
            .checked_add(count)
            .ok_or_else(|| overflow(context))?;
        limit("records", self.records, self.limits.max_records)?;
        Ok(count)
    }
    fn list(&mut self, minimum: usize, context: &'static str) -> Result<usize, ParseError> {
        let count = self.count(context)?;
        self.require(product(count, minimum, context)?, context)?;
        Ok(count)
    }
}

fn read<'a>(
    input: &'a [u8],
    q: u32,
    limits: &Limits,
    store: bool,
) -> Result<PackedV20<'a>, ParseError> {
    let mut r = Reader {
        source: input,
        offset: 0,
        names: 0,
        records: 0,
        limits,
    };
    let version = r.word("version")?;
    if version != 20 {
        return Err(ParseError::UnsupportedVersion { version });
    }
    if q == 0 {
        return Err(ParseError::InvalidDimensions { quads_per_tile: q });
    }
    let dimension = usize::try_from(q).map_err(|_| overflow("tile dimensions"))?;
    let side = dimension
        .checked_add(1)
        .ok_or_else(|| overflow("tile dimensions"))?;
    let n = product(side, side, "tile dimensions")?;
    let grid_size = product(n, 4, "tile dimensions")?;
    let quad_size = product(dimension, dimension, "tile dimensions")?;
    let minimum_tile = product(grid_size, 3, "tile dimensions")?
        .checked_add(quad_size)
        .and_then(|bytes| bytes.checked_add(40))
        .ok_or_else(|| overflow("tile dimensions"))?;
    let words = [version, r.word("header words")?, r.word("header words")?];
    let name = r.name("header name")?;
    let tile_count = r.word("tile count")?;
    let count = usize::try_from(tile_count).map_err(|_| overflow("tile count"))?;
    limit("tiles", count, limits.max_tiles)?;
    let header = Header {
        source_range: 0..r.offset,
        words,
        name,
        tile_count,
    };
    r.require(product(count, minimum_tile, "tiles")?, "tiles")?;
    let mut tiles = Vec::new();
    reserve(&mut tiles, count, "tiles", store)?;
    for _ in 0..count {
        let start = r.offset;
        let coordinates = [r.word("tile coordinates")?, r.word("tile coordinates")?];
        let identifier = r.word("tile identifier")?;
        let grid_bytes = [
            r.take(grid_size, "grid 0")?,
            r.take(grid_size, "grid 1")?,
            r.take(grid_size, "grid 2")?,
        ];
        let quad_bytes = r.take(quad_size, "quad bytes")?;
        let prefix_words = [r.word("tile prefix words")?, r.word("tile prefix words")?];
        let layer_count = r.count("layers")?;
        let layer_minimum = if layer_count == 0 {
            0
        } else {
            product(layer_count - 1, 5, "layers")?
                .checked_add(1)
                .ok_or_else(|| overflow("layers"))?
        };
        r.require(layer_minimum, "layers")?;
        let mut layers = Vec::new();
        reserve(&mut layers, layer_count, "layers", store)?;
        for index in 0..layer_count {
            let start = r.offset;
            let name = r.name("layer name")?;
            let coverage = if index > 0 {
                let start = r.offset;
                let resolution = r.word("coverage resolution")?;
                let side = usize::try_from(resolution).map_err(|_| overflow("coverage bytes"))?;
                let bytes = r.take(product(side, side, "coverage bytes")?, "coverage bytes")?;
                Some(Coverage {
                    source_range: start..r.offset,
                    resolution,
                    bytes,
                })
            } else {
                None
            };
            if store {
                layers.push(Layer {
                    source_range: start..r.offset,
                    name,
                    coverage,
                });
            }
        }
        let count_a = r.list(47, "section A")?;
        let mut section_a = Vec::new();
        reserve(&mut section_a, count_a, "section A", store)?;
        for _ in 0..count_a {
            let start = r.offset;
            let name1 = r.name("section A name1")?;
            let name2 = r.name("section A name2")?;
            let blob = r.take(44, "section A blob")?;
            let byte = r.byte("section A byte")?;
            if store {
                section_a.push(RecordA {
                    source_range: start..r.offset,
                    name1,
                    name2,
                    blob,
                    byte,
                });
            }
        }
        let count_b = r.list(62, "section B")?;
        let mut section_b = Vec::new();
        reserve(&mut section_b, count_b, "section B", store)?;
        for _ in 0..count_b {
            let start = r.offset;
            let name1 = r.name("section B name1")?;
            let word = r.word("section B word")?;
            let name2 = r.name("section B name2")?;
            let blob = r.take(56, "section B blob")?;
            if store {
                section_b.push(RecordB {
                    source_range: start..r.offset,
                    name1,
                    word,
                    name2,
                    blob,
                });
            }
        }
        let count_c = r.list(51, "section C")?;
        let mut section_c = Vec::new();
        reserve(&mut section_c, count_c, "section C", store)?;
        for _ in 0..count_c {
            let start = r.offset;
            let name1 = r.name("section C name1")?;
            let name2 = r.name("section C name2")?;
            let byte = r.byte("section C byte")?;
            let blob = r.take(48, "section C blob")?;
            if store {
                section_c.push(RecordC {
                    source_range: start..r.offset,
                    name1,
                    name2,
                    byte,
                    blob,
                });
            }
        }
        let count_d = r.list(49, "section D")?;
        let mut section_d = Vec::new();
        reserve(&mut section_d, count_d, "section D", store)?;
        for _ in 0..count_d {
            let start = r.offset;
            let name = r.name("section D name")?;
            let blob = r.take(48, "section D blob")?;
            if store {
                section_d.push(RecordD {
                    source_range: start..r.offset,
                    name,
                    blob,
                });
            }
        }
        if store {
            tiles.push(TileV20 {
                source_range: start..r.offset,
                coordinates,
                identifier,
                grid_bytes,
                quad_bytes,
                prefix_words,
                layers,
                section_a,
                section_b,
                section_c,
                section_d,
            });
        }
    }
    Ok(PackedV20 {
        source: input,
        header,
        quads_per_tile: q,
        tiles,
        consumed_len: r.offset,
    })
}
