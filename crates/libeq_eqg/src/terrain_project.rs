//! Borrowed EQTZP project-text tokens without field or terrain interpretation.
//!
//! Source spelling, separators and any opaque bytes after the first NUL remain
//! recoverable. A successful parse establishes text framing only.

/// A failure to establish the supported project-text header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("invalid EQTZP magic")]
    InvalidMagic,
    #[error("expected a complete EQTZP header token at byte {offset}")]
    InvalidHeaderToken { offset: usize },
}

/// Immutable, allocation-free borrowed view validated by [`parse`].
#[derive(Debug, Clone, Copy)]
pub struct Project<'a> {
    source: &'a [u8],
    end: usize,
}

/// One token, including its original position and untrimmed spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token<'a> {
    /// Zero-based byte offset of the first token byte in [`Project::source`].
    pub byte_offset: usize,
    /// Bytes before SPACE, TAB, LF, NUL or EOF, after skipping leading whitespace.
    /// Trailing CR, VT and FF remain in this slice.
    pub raw: &'a [u8],
    /// `raw` with trailing ASCII C whitespace removed; interior bytes are unchanged.
    pub bytes: &'a [u8],
}

impl<'a> Project<'a> {
    /// The complete original input, including separators, NUL and any suffix.
    pub fn source(&self) -> &'a [u8] {
        self.source
    }

    /// Offset of the first NUL, or `None` when the text extends through EOF.
    pub fn nul_offset(&self) -> Option<usize> {
        (self.end < self.source.len()).then_some(self.end)
    }

    /// Bytes after the first NUL, excluding that terminator and interpreting nothing.
    /// `None` means no terminator; `Some(b"")` means a terminator at EOF.
    pub fn trailing_data(&self) -> Option<&'a [u8]> {
        self.nul_offset().map(|offset| &self.source[offset + 1..])
    }

    /// Iterate tokens in original order, including the header, without allocation.
    ///
    /// Leading TAB, LF, VT, FF, CR and SPACE are skipped. Only SPACE, TAB and LF
    /// separate collected tokens; trailing bytes from the six-byte whitespace
    /// set are trimmed in [`Token::bytes`]. Interior CR, VT and FF are preserved.
    /// Iteration stops before the first NUL. Each call starts an independent
    /// linear pass and borrows the input, not this project handle.
    pub fn tokens(&self) -> impl Iterator<Item = Token<'a>> + 'a + use<'a> {
        let source = &self.source[..self.end];
        let mut cursor = 0;
        std::iter::from_fn(move || {
            while cursor < source.len() && whitespace(source[cursor]) {
                cursor += 1;
            }
            if cursor == source.len() {
                return None;
            }
            let start = cursor;
            while cursor < source.len() && !matches!(source[cursor], b' ' | b'\t' | b'\n') {
                cursor += 1;
            }
            let raw = &source[start..cursor];
            let mut trimmed = raw.len();
            while trimmed > 0 && whitespace(raw[trimmed - 1]) {
                trimmed -= 1;
            }
            let bytes = &raw[..trimmed];
            Some(Token {
                byte_offset: start,
                raw,
                bytes,
            })
        })
    }
}

/// Validate an exact `EQTZP` first token at byte zero, borrowing all input bytes.
///
/// Header prefix continuations are refused by this library even though
/// [`crate::identify`] recognizes the five-byte signature alone. No newline,
/// required field, version, UTF-8, numeric value or key ordering is required.
/// Unknown keys, repeated keys, punctuation and quotes are ordinary token bytes.
/// The first NUL terminates text; any later bytes remain an opaque suffix.
///
/// Parsing and each token pass take linear time and O(1) extra storage, without
/// allocation or token truncation. Input storage, extraction and repeated caller
/// passes are outside that bound. This is not DAT decoding, dependency discovery,
/// consumer defaults, semantic validation or terrain/runtime admission.
///
/// ```
/// use libeq_eqg::terrain_project;
/// let input = b"EQTZP\r\n*NAME Example\0opaque";
/// let project = terrain_project::parse(input)?;
/// let name = project.tokens().nth(2).unwrap();
/// assert_eq!(name.byte_offset, 13);
/// assert_eq!(name.bytes, b"Example");
/// assert_eq!(project.nul_offset(), Some(20));
/// assert_eq!(project.trailing_data(), Some(b"opaque".as_slice()));
/// assert_eq!(project.source(), input);
/// # Ok::<(), terrain_project::ParseError>(())
/// ```
pub fn parse(input: &[u8]) -> Result<Project<'_>, ParseError> {
    if !input.starts_with(b"EQTZP") {
        return Err(ParseError::InvalidMagic);
    }
    let end = input
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(input.len());
    let project = Project { source: input, end };
    if project.tokens().next().map(|token| token.bytes) != Some(b"EQTZP".as_slice()) {
        return Err(ParseError::InvalidHeaderToken { offset: 5 });
    }
    Ok(project)
}

fn whitespace(byte: u8) -> bool {
    matches!(byte, b'\t' | b'\n' | 0x0b | 0x0c | b'\r' | b' ')
}
