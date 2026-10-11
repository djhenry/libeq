//! Inspect one explicitly selected archived classic EQGP resource; no filename inference.
use libeq_eqg::lighting;
use libeq_pfs::PfsReader;
use std::{error::Error, fs::File, io};
fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let archive = args.next().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: lighting_probe ARCHIVE MEMBER",
        )
    })?;
    let member = args
        .next()
        .and_then(|s| s.into_string().ok())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "member must be a UTF-8 argument",
            )
        })?;
    if args.next().is_some() {
        return Err(
            io::Error::new(io::ErrorKind::InvalidInput, "unexpected extra argument").into(),
        );
    }
    let mut archive = PfsReader::open(File::open(archive)?)?;
    let input = archive.get(&member)?.ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "selected archive member is absent")
    })?;
    let parsed = lighting::parse(&input)?;
    // Independently compare the borrowed word view with the counted source bytes.
    let count = u32::from_le_bytes(input[4..8].try_into()?);
    let count = usize::try_from(count)?;
    let end = 8usize
        .checked_add(count.checked_mul(4).ok_or("count overflow")?)
        .ok_or("offset overflow")?;
    assert_eq!(parsed.sample_count() as usize, count);
    assert_eq!(parsed.packed_bytes(), &input[8..end]);
    let words = parsed.packed_words();
    assert_eq!(
        words.len(),
        count,
        "iterator length differs from declared count"
    );
    verify_packed_words(words, &input[8..end]);
    assert_eq!(parsed.trailing_data(), &input[end..]);
    println!(
        "samples={} packed_bytes={} trailing_bytes={} first={:?} last={:?}",
        parsed.sample_count(),
        parsed.packed_bytes().len(),
        parsed.trailing_data().len(),
        parsed.get(0),
        parsed.packed_words().next_back()
    );
    Ok(())
}

fn verify_packed_words(mut words: impl Iterator<Item = u32>, source: &[u8]) {
    let chunks = source.chunks_exact(4);
    assert!(chunks.remainder().is_empty(), "source word is incomplete");
    for b in chunks {
        assert_eq!(
            words.next(),
            Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            "packed word differs or is missing"
        );
    }
    assert!(words.next().is_none(), "extra packed words");
}

#[cfg(test)]
mod tests {
    use super::verify_packed_words;
    const SOURCE: &[u8] = &[4, 3, 2, 1, 0xdd, 0xcc, 0xbb, 0xaa];
    #[test]
    fn verifies_complete_and_zero_word_streams() {
        verify_packed_words([0x01020304, 0xaabbccdd].into_iter(), SOURCE);
        verify_packed_words(std::iter::empty(), &[]);
    }
    #[test]
    #[should_panic(expected = "packed word differs or is missing")]
    fn rejects_empty_iterator_for_nonempty_source() {
        verify_packed_words(std::iter::empty(), SOURCE);
    }
    #[test]
    #[should_panic(expected = "packed word differs or is missing")]
    fn rejects_short_iterator() {
        verify_packed_words([0x01020304].into_iter(), SOURCE);
    }
    #[test]
    #[should_panic(expected = "extra packed words")]
    fn rejects_extra_iterator_word() {
        verify_packed_words([0x01020304, 0xaabbccdd, 0x12345678].into_iter(), SOURCE);
    }
}
