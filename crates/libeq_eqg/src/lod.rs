//! Borrowed raw EQLOD rows, without model selection or numeric interpretation.
//!
//! Names, number tokens, unknown rows, and duplicates remain unchanged. A parsed
//! list establishes framing only; it does not admit names or numbers for runtime use.

/// A failure to establish the supported text framing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("invalid EQLOD magic")]
    InvalidMagic,
    #[error("expected CRLF or LF after EQLOD at byte {offset}")]
    InvalidHeaderEnding { offset: usize },
    #[error("NUL byte in EQLOD body at byte {offset}")]
    InteriorNul { offset: usize },
}

/// Immutable, allocation-free view validated by [`parse`].
#[derive(Debug, Clone, Copy)]
pub struct LodList<'a> {
    source: &'a [u8],
    body: &'a [u8],
}

/// One physical body row, retaining its original location and contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Record<'a> {
    /// Zero-based byte offset into [`LodList::source`].
    pub byte_offset: usize,
    /// One-based line number; the header is line 1.
    pub line_number: usize,
    /// Original row bytes excluding LF and at most one trailing CR, even at EOF.
    pub raw_line: &'a [u8],
    pub kind: RecordKind<'a>,
}

/// Raw row classification, with case-insensitive ASCII tags and no leading trim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordKind<'a> {
    Level {
        /// Bytes between `LOD,` and the next comma, or the entire remainder.
        model: &'a [u8],
        /// Every byte after that comma, including any further commas.
        /// `None` means no comma followed the model; `Some(b"")` means an empty
        /// token. These bytes are unvalidated, not a parsed or finite number.
        raw_number: Option<&'a [u8]>,
    },
    Collision {
        /// The entire remainder after `COL,`, including any commas.
        model: &'a [u8],
    },
    /// Unrecognized or blank row; its bytes are available in [`Record::raw_line`].
    Unknown,
}

impl<'a> LodList<'a> {
    /// Original bytes, including the header and all line endings.
    pub fn source(&self) -> &'a [u8] {
        self.source
    }

    /// Iterate all body rows in source order without allocation or conversion.
    ///
    /// A terminal LF produces no extra row; physical blank rows are retained.
    /// Each call starts an independent linear pass. Repeated caller passes are
    /// not bounded by this view, and no total process-memory bound is implied.
    pub fn records(&self) -> impl Iterator<Item = Record<'a>> + 'a {
        let mut remaining = self.body;
        let mut byte_offset = self.source.len() - self.body.len();
        let mut line_number = 2;
        std::iter::from_fn(move || {
            if remaining.is_empty() {
                return None;
            }
            let end = remaining
                .iter()
                .position(|&byte| byte == b'\n')
                .unwrap_or(remaining.len());
            let line = &remaining[..end];
            let raw_line = line.strip_suffix(b"\r").unwrap_or(line);
            let kind = if raw_line
                .get(..4)
                .is_some_and(|tag| tag.eq_ignore_ascii_case(b"LOD,"))
            {
                let rest = &raw_line[4..];
                let (model, raw_number) = match rest.iter().position(|&byte| byte == b',') {
                    Some(comma) => (&rest[..comma], Some(&rest[comma + 1..])),
                    None => (rest, None),
                };
                RecordKind::Level { model, raw_number }
            } else if raw_line
                .get(..4)
                .is_some_and(|tag| tag.eq_ignore_ascii_case(b"COL,"))
            {
                RecordKind::Collision {
                    model: &raw_line[4..],
                }
            } else {
                RecordKind::Unknown
            };
            let record = Record {
                byte_offset,
                line_number,
                raw_line,
                kind,
            };
            let consumed = end + usize::from(end < remaining.len());
            remaining = &remaining[consumed..];
            byte_offset += consumed;
            line_number += 1;
            Some(record)
        })
    }
}

/// Validate an exact `EQLOD` header followed by CRLF or LF, borrowing all bytes.
///
/// Bare LF is a library extension. The body may end without LF. A trailing CR
/// is excluded from the final row just as it is before LF; embedded CR remains.
/// NUL anywhere in the body is an explicit library framing refusal with the
/// original byte offset, rather than C-string truncation. Header errors take
/// precedence over body validation. Unknown, blank, empty-name and duplicate rows
/// are retained, without sorting, normalization, numeric defaults or validation.
/// Standard byte-slice `eq_ignore_ascii_case` can compare names without copying.
///
/// Parsing and each iterator pass take linear time and allocate nothing. The
/// caller owns input storage and archive extraction; no native-runtime parity,
/// distance units, threshold comparison or model-resolution behavior is implied.
///
/// ```
/// use libeq_eqg::lod::{self, RecordKind};
/// let input = b"EQLOD\r\nLOD,Example,00300\r\nCOL,Example,collision";
/// let list = lod::parse(input)?;
/// let rows: Vec<_> = list.records().collect();
/// assert_eq!(rows[0].byte_offset, 7);
/// assert_eq!(rows[0].line_number, 2);
/// assert_eq!(rows[0].kind, RecordKind::Level {
///     model: b"Example", raw_number: Some(b"00300"),
/// });
/// assert_eq!(rows[1].kind, RecordKind::Collision { model: b"Example,collision" });
/// assert_eq!(list.source(), input);
/// # Ok::<(), lod::ParseError>(())
/// ```
pub fn parse(input: &[u8]) -> Result<LodList<'_>, ParseError> {
    if !input.starts_with(b"EQLOD") {
        return Err(ParseError::InvalidMagic);
    }
    let header_length = if input.get(5..7) == Some(b"\r\n") {
        7
    } else if input.get(5) == Some(&b'\n') {
        6
    } else {
        return Err(ParseError::InvalidHeaderEnding { offset: 5 });
    };
    if let Some(offset) = input.iter().position(|&byte| byte == 0) {
        return Err(ParseError::InteriorNul { offset });
    }
    Ok(LodList {
        source: input,
        body: &input[header_length..],
    })
}
