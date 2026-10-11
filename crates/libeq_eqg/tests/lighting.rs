use libeq_eqg::{
    Error, FormatHeader, identify,
    lighting::{self, Limits, ParseError},
};
fn record(words: &[u32], suffix: &[u8]) -> Vec<u8> {
    let mut bytes = b"EQGP".to_vec();
    bytes.extend((words.len() as u32).to_le_bytes());
    for word in words {
        bytes.extend(word.to_le_bytes());
    }
    bytes.extend(suffix);
    bytes
}
#[test]
fn versionless_header_words_preserve_all_bits_and_suffix() {
    let words = [0x0001_0203, 0xaa44_5566, 0xff00_00ff, 0x8000_0000, u32::MAX];
    let bytes = record(&words, b"\xffopaque\0\x01");
    let parsed = lighting::parse(&bytes).unwrap();
    assert_eq!(parsed.sample_count(), 5);
    assert_eq!(parsed.packed_words().collect::<Vec<_>>(), words);
    assert_eq!(parsed.packed_bytes(), &bytes[8..28]);
    assert_eq!(parsed.trailing_data(), b"\xffopaque\0\x01");
    assert_eq!(parsed.packed_bytes().as_ptr(), bytes[8..].as_ptr());
    assert_eq!(parsed.trailing_data().as_ptr(), bytes[28..].as_ptr());
}
#[test]
fn zero_count_and_zero_limits_keep_opaque_suffix() {
    let bytes = record(&[], b"uninterpreted");
    let parsed = lighting::parse_with_limits(
        &bytes,
        &Limits {
            max_sample_count: 0,
            max_payload_bytes: 0,
        },
    )
    .unwrap();
    assert_eq!(parsed.sample_count(), 0);
    assert_eq!(parsed.packed_words().len(), 0);
    assert_eq!(parsed.get(0), None);
    assert_eq!(parsed.trailing_data(), b"uninterpreted");
}
#[test]
fn every_truncation_point_is_checked_without_panicking() {
    let bytes = record(&[0x01020304, 0xffeeddcc], &[]);
    for end in 0..bytes.len() {
        let err = lighting::parse(&bytes[..end]).unwrap_err();
        assert!(
            matches!(err, ParseError::Truncated { .. }),
            "end{end}: {err:?}"
        );
    }
    assert_eq!(
        lighting::parse(&bytes[..7]).unwrap_err(),
        ParseError::Truncated {
            context: "sample count",
            offset: 4,
            needed: 4,
            remaining: 3
        }
    );
    assert_eq!(
        lighting::parse(&bytes[..15]).unwrap_err(),
        ParseError::Truncated {
            context: "packed samples",
            offset: 8,
            needed: 8,
            remaining: 7
        }
    );
    assert_eq!(lighting::parse(&bytes).unwrap().sample_count(), 2);
}
#[test]
fn unknown_or_headerless_data_is_not_accepted() {
    for input in [
        b"EQGQ\0\0\0\0".as_slice(),
        b"eqgp\0\0\0\0",
        b"\x02\0\0\x80\x02\0\0\x80",
        b"\xef\xbb\xbfEQGP\0\0\0\0",
    ] {
        assert_eq!(
            lighting::parse(input).unwrap_err(),
            ParseError::InvalidMagic
        );
        assert_eq!(identify(input), Ok(None));
    }
}
#[test]
fn limits_have_exact_word_and_payload_boundaries_before_sample_access() {
    let bytes = record(&[1, 2, 3], b"suffix beyond payload cap");
    let limits = Limits {
        max_sample_count: 3,
        max_payload_bytes: 12,
    };
    assert_eq!(
        lighting::parse_with_limits(&bytes, &limits)
            .unwrap()
            .sample_count(),
        3
    );
    assert_eq!(
        lighting::parse_with_limits(
            &bytes,
            &Limits {
                max_sample_count: 2,
                ..limits
            }
        )
        .unwrap_err(),
        ParseError::SampleLimit {
            sample_count: 3,
            limit: 2
        }
    );
    assert_eq!(
        lighting::parse_with_limits(
            &bytes,
            &Limits {
                max_payload_bytes: 11,
                ..limits
            }
        )
        .unwrap_err(),
        ParseError::PayloadLimit {
            bytes: 12,
            limit: 11
        }
    );
    let mut header = b"EQGP".to_vec();
    header.extend(4u32.to_le_bytes());
    assert_eq!(
        lighting::parse_with_limits(&header, &limits).unwrap_err(),
        ParseError::SampleLimit {
            sample_count: 4,
            limit: 3
        }
    );
    assert_eq!(
        lighting::parse_with_limits(
            &header,
            &Limits {
                max_sample_count: 4,
                max_payload_bytes: 15
            }
        )
        .unwrap_err(),
        ParseError::PayloadLimit {
            bytes: 16,
            limit: 15
        }
    );
}
#[test]
fn huge_counts_and_default_caps_fail_before_payload_iteration() {
    let limits = Limits::default();
    assert_eq!(limits.max_sample_count, 16_777_216);
    assert_eq!(limits.max_payload_bytes, 64 * 1024 * 1024);
    let mut header = b"EQGP".to_vec();
    header.extend(u32::MAX.to_le_bytes());
    assert!(matches!(
        lighting::parse(&header),
        Err(ParseError::SampleLimit { .. }) | Err(ParseError::CountOverflow)
    ));
    let mut at_limit = b"EQGP".to_vec();
    at_limit.extend(16_777_216u32.to_le_bytes());
    assert_eq!(
        lighting::parse(&at_limit).unwrap_err(),
        ParseError::Truncated {
            context: "packed samples",
            offset: 8,
            needed: 64 * 1024 * 1024,
            remaining: 0
        }
    );
    let mut header = b"EQGP".to_vec();
    header.extend(16_777_217u32.to_le_bytes());
    assert_eq!(
        lighting::parse(&header).unwrap_err(),
        ParseError::SampleLimit {
            sample_count: 16_777_217,
            limit: 16_777_216
        }
    );
}
#[test]
fn nonaligned_borrowed_input_has_checked_random_access_and_exact_iteration() {
    let words = [0x10203040, 0x01020304, 0xaabbccdd];
    let bytes = record(&words, &[5]);
    let mut storage = vec![0; bytes.len() + 4];
    let offset = (5 - (storage.as_ptr() as usize % 4)) % 4;
    storage[offset..offset + bytes.len()].copy_from_slice(&bytes);
    let parsed = lighting::parse(&storage[offset..offset + bytes.len()]).unwrap();
    assert_eq!(parsed.packed_bytes().as_ptr() as usize % 4, 1);
    assert_eq!(parsed.get(0), Some(words[0]));
    assert_eq!(parsed.get(2), Some(words[2]));
    assert_eq!(parsed.get(3), None);
    assert_eq!(parsed.get(usize::MAX), None);
    assert_eq!(parsed.get(usize::MAX / 4 + 1), None);
    let mut iter = parsed.packed_words();
    assert_eq!(iter.size_hint(), (3, Some(3)));
    assert_eq!(iter.next(), Some(words[0]));
    assert_eq!(iter.len(), 2);
    assert_eq!(iter.next_back(), Some(words[2]));
    assert_eq!(iter.clone().collect::<Vec<_>>(), [words[1]]);
    assert_eq!(iter.next(), Some(words[1]));
    assert_eq!(iter.len(), 0);
    assert_eq!(iter.next_back(), None);
}
#[test]
fn identify_reports_sample_count_without_assuming_version_or_valid_payload() {
    for count in [0, 1, 234, 0x04030201, u32::MAX] {
        let mut input = b"EQGP".to_vec();
        input.extend(count.to_le_bytes());
        assert_eq!(
            identify(&input),
            Ok(Some(FormatHeader::Lighting {
                sample_count: count
            }))
        );
        input.extend(b"not validated here");
        assert_eq!(
            identify(&input),
            Ok(Some(FormatHeader::Lighting {
                sample_count: count
            }))
        );
    }
}
#[test]
fn identify_requires_complete_versionless_header() {
    let bytes = b"EQGP\x01\x02\x03\x04";
    for end in 1..8 {
        assert_eq!(
            identify(&bytes[..end]),
            Err(Error::TruncatedHeader {
                expected: if end < 4 { 4 } else { 8 },
                actual: end
            })
        );
    }
}

#[test]
fn explicit_large_limits_still_reject_missing_counted_data_without_allocating() {
    let mut bytes = b"EQGP".to_vec();
    bytes.extend(u32::MAX.to_le_bytes());
    let limits = Limits {
        max_sample_count: usize::MAX,
        max_payload_bytes: usize::MAX,
    };
    let error = lighting::parse_with_limits(&bytes, &limits).unwrap_err();
    match usize::try_from(u64::from(u32::MAX) * 4) {
        Err(_) => assert_eq!(error, ParseError::CountOverflow),
        Ok(needed) => assert_eq!(
            error,
            ParseError::Truncated {
                context: "packed samples",
                offset: 8,
                needed,
                remaining: 0
            }
        ),
    }
}
