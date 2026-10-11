//! Classic versionless EQGP packed lighting samples, retained without interpretation.
//! Headerless data, color conventions and resource association are not inferred.

/// Bounds on counted packed words, not the total file or process memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_sample_count: usize,
    /// Counted packed samples only; the opaque suffix is borrowed without copying.
    pub max_payload_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_sample_count: 16_777_216,
            max_payload_bytes: 64 * 1024 * 1024,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("invalid EQGP magic")]
    InvalidMagic,
    #[error("truncated {context} at byte {offset}: need {needed} bytes, have {remaining}")]
    Truncated {
        context: &'static str,
        offset: usize,
        needed: usize,
        remaining: usize,
    },
    #[error("EQGP sample byte count overflow")]
    CountOverflow,
    #[error("EQGP sample count {sample_count} exceeds limit {limit}")]
    SampleLimit { sample_count: u32, limit: usize },
    #[error("EQGP packed payload {bytes} bytes exceeds limit {limit}")]
    PayloadLimit { bytes: usize, limit: usize },
}
/// Immutable validated view. Private fields keep the declared count and word view consistent.
/// Words are read from bytes, with no alignment requirement.
#[derive(Debug, Clone, Copy)]
pub struct Lighting<'a> {
    samples: &'a [u8],
    suffix: &'a [u8],
}
impl<'a> Lighting<'a> {
    /// The count from the versionless header, matching the validated byte view.
    pub fn sample_count(&self) -> u32 {
        (self.samples.len() / 4) as u32
    }
    /// Borrow the declared packed bytes, excluding the header and opaque suffix.
    pub fn packed_bytes(&self) -> &'a [u8] {
        self.samples
    }
    /// Iterate raw little-endian words without decoding channels or other meanings.
    pub fn packed_words(
        &self,
    ) -> impl ExactSizeIterator<Item = u32> + DoubleEndedIterator + Clone + 'a {
        self.samples
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    /// Read one raw word; out-of-range and overflowing indices return `None`.
    pub fn get(&self, index: usize) -> Option<u32> {
        let start = index.checked_mul(4)?;
        let end = start.checked_add(4)?;
        let b = self.samples.get(start..end)?;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    /// All bytes following the declared words; retained without interpreting them.
    pub fn trailing_data(&self) -> &'a [u8] {
        self.suffix
    }
}
/// Parse a classic EQGP record with the default counted-payload limits.
///
/// ```
/// use libeq_eqg::lighting;
/// let bytes = b"EQGP\x01\0\0\0\x04\x03\x02\x01suffix";
/// let samples = lighting::parse(bytes)?;
/// assert_eq!(samples.sample_count(), 1);
/// assert_eq!(samples.get(0), Some(0x01020304));
/// assert_eq!(samples.trailing_data(), b"suffix");
/// # Ok::<(), lighting::ParseError>(())
/// ```
pub fn parse(input: &[u8]) -> Result<Lighting<'_>, ParseError> {
    parse_with_limits(input, &Limits::default())
}
/// An eight-byte header is magic plus sample count, with no version word.
/// Count arithmetic and limits are checked before obtaining or iterating samples.
pub fn parse_with_limits<'a>(input: &'a [u8], limits: &Limits) -> Result<Lighting<'a>, ParseError> {
    if take(input, 0, 4, "magic")? != b"EQGP" {
        return Err(ParseError::InvalidMagic);
    }
    let raw = take(input, 4, 4, "sample count")?;
    let sample_count = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
    let count = usize::try_from(sample_count).map_err(|_| ParseError::CountOverflow)?;
    let bytes = count.checked_mul(4).ok_or(ParseError::CountOverflow)?;
    if count > limits.max_sample_count {
        return Err(ParseError::SampleLimit {
            sample_count,
            limit: limits.max_sample_count,
        });
    }
    if bytes > limits.max_payload_bytes {
        return Err(ParseError::PayloadLimit {
            bytes,
            limit: limits.max_payload_bytes,
        });
    }
    let samples = take(input, 8, bytes, "packed samples")?;
    let end = 8usize.checked_add(bytes).ok_or(ParseError::CountOverflow)?;
    Ok(Lighting {
        samples,
        suffix: &input[end..],
    })
}
fn take<'a>(
    input: &'a [u8],
    offset: usize,
    needed: usize,
    context: &'static str,
) -> Result<&'a [u8], ParseError> {
    let end = offset
        .checked_add(needed)
        .ok_or(ParseError::CountOverflow)?;
    input.get(offset..end).ok_or(ParseError::Truncated {
        context,
        offset,
        needed,
        remaining: input.len().saturating_sub(offset),
    })
}
