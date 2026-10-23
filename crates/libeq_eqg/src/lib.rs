//! Identify EverQuest EQG resources and parse raw resource records.
//!
//! [`identify`] inspects headers only; [`zone::parse`] validates binary EQGZ
//! version 1 and 2 descriptors. [`mesh`] retains geometry; [`lighting`] reads
//! classic versionless EQGP packed samples. [`terrain_project`] borrows EQTZP
//! text tokens without interpreting project fields. Archive extraction belongs to `libeq_pfs`.

pub mod lighting;
pub mod terrain_project;
pub mod zone;

/// A recognized format header: raw versions for zones/meshes, a count for EQGP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatHeader {
    /// Binary zone descriptor (`EQGZ`).
    Zone { version: u32 },
    /// Binary terrain mesh (`EQGT`).
    Terrain { version: u32 },
    /// Binary model mesh (`EQGM`).
    Model { version: u32 },
    /// An `EQGP` signature and its raw count word; payload support is not established.
    Lighting { sample_count: u32 },
    /// Text terrain project (`EQTZP`); no binary version is read.
    TerrainProject,
    /// Text object group (`EQOBG`); no binary version is read.
    ObjectGroup,
}

/// A recognized header prefix whose remaining bytes are missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("truncated EQG header: expected at least {expected} bytes, got {actual}")]
    TruncatedHeader {
        /// Minimum total length needed for the signature, or eight bytes once a
        /// complete binary signature establishes that a header word must follow.
        expected: usize,
        /// Number of input bytes available.
        actual: usize,
    },
}

/// Identify a resource from its signature at byte zero.
///
/// Zone/mesh versions and the classic EQGP sample count are little-endian `u32`
/// header words returned unchanged. The classic EQGP layout has no version field.
/// Text signatures have neither.
/// Trailing data is ignored; this function does not validate a complete file.
/// In particular, an EQGP header may be recognized even when [`lighting::parse`]
/// rejects its payload. Identification cannot distinguish a damaged classic
/// record from an unsupported encoding using the same signature.
///
/// Returns `Ok(None)` for empty input or an unknown signature. A nonempty prefix
/// of a known signature, or a binary signature with fewer than four header-word
/// bytes, returns [`Error::TruncatedHeader`]. Signatures are case-sensitive;
/// whitespace and byte-order marks are not skipped.
///
/// ```
/// use libeq_eqg::{FormatHeader, identify};
/// assert_eq!(identify(b"EQGZ\x02\0\0\0"),
///            Ok(Some(FormatHeader::Zone { version: 2 })));
/// assert_eq!(identify(b"unrecognized"), Ok(None));
/// ```
pub fn identify(input: &[u8]) -> Result<Option<FormatHeader>, Error> {
    if input.is_empty() {
        return Ok(None);
    }

    const SIGNATURES: [&[u8]; 6] = [b"EQGZ", b"EQGT", b"EQGM", b"EQTZP", b"EQOBG", b"EQGP"];
    for magic in SIGNATURES {
        if input.len() < magic.len() && magic.starts_with(input) {
            return Err(Error::TruncatedHeader {
                expected: magic.len(),
                actual: input.len(),
            });
        }
        if !input.starts_with(magic) {
            continue;
        }
        if magic == b"EQTZP" {
            return Ok(Some(FormatHeader::TerrainProject));
        }
        if magic == b"EQOBG" {
            return Ok(Some(FormatHeader::ObjectGroup));
        }
        let header_bytes = input.get(4..8).ok_or(Error::TruncatedHeader {
            expected: 8,
            actual: input.len(),
        })?;
        let word = u32::from_le_bytes([
            header_bytes[0],
            header_bytes[1],
            header_bytes[2],
            header_bytes[3],
        ]);
        return Ok(Some(match magic {
            b"EQGZ" => FormatHeader::Zone { version: word },
            b"EQGT" => FormatHeader::Terrain { version: word },
            b"EQGP" => FormatHeader::Lighting { sample_count: word },
            _ => FormatHeader::Model { version: word },
        }));
    }
    Ok(None)
}

/// Raw terrain and model geometry.
pub mod mesh;
