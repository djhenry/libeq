use libeq_wld::parser::{Fragment, FragmentParser, FragmentRef, Region, WldDoc};

fn minimal_region(flags: i32, references: &[i32]) -> Vec<u8> {
    // Twelve words: name, flags, ambient, eight counts, and userdata length.
    let header: [i32; 12] = [0, flags, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    header
        .into_iter()
        .chain(references.iter().copied())
        .flat_map(i32::to_le_bytes)
        .collect()
}

#[test]
fn modern_reference_follows_the_0x40_reference() {
    let bytes = minimal_region(0x140, &[111, 222]);
    let (remaining, region) = Region::parse(&bytes).unwrap();
    assert!(matches!(
        region.mesh_reference,
        Some(FragmentRef::Index(222, _))
    ));
    assert!(remaining.is_empty());
}

#[test]
fn reference_flags_are_independent() {
    let cases = &[
        (0, &[][..], None, None),
        (0x40, &[111], Some(111), None),
        (0x100, &[222], None, Some(222_i32)),
        (0x140, &[111, 222], Some(111), Some(222)),
    ];
    for &(flags, words, earlier, modern) in cases {
        let bytes = minimal_region(flags, words);
        let (remaining, region) = Region::parse(&bytes).unwrap();
        assert_eq!(region.reference_0x40, earlier.map(FragmentRef::new));
        assert_eq!(
            region.mesh_reference.as_ref().map(FragmentRef::to_bytes),
            modern.map(|value| value.to_le_bytes().to_vec())
        );
        assert_eq!(region.flags.has_reference_0x40(), earlier.is_some());
        assert_eq!(region.flags.has_mesh_reference(), modern.is_some());
        assert!(remaining.is_empty());
        assert_eq!(region.to_bytes(), bytes);
        let serialized = region.to_bytes();
        let (remaining, reparsed) = Region::parse(&serialized).unwrap();
        assert!(remaining.is_empty());
        assert_eq!(reparsed, region);
    }
}

#[test]
fn signed_null_and_extreme_references_are_preserved_without_resolution() {
    for (earlier, modern) in [
        (0, -1),
        (-1, 0),
        (-111, -222),
        (i32::MIN, i32::MAX),
        (i32::MAX, i32::MIN),
    ] {
        let bytes = minimal_region(0x140, &[earlier, modern]);
        let (remaining, region) = Region::parse(&bytes).unwrap();
        assert!(remaining.is_empty());
        assert_eq!(region.reference_0x40, Some(FragmentRef::new(earlier)));
        assert_eq!(
            region.mesh_reference.as_ref().unwrap().to_bytes(),
            modern.to_le_bytes()
        );
        assert_eq!(region.to_bytes(), bytes);
    }
}

#[test]
fn each_truncated_optional_reference_is_an_error() {
    for (flags, references) in [
        (0x40, vec![111]),
        (0x100, vec![222]),
        (0x140, vec![111, 222]),
    ] {
        let bytes = minimal_region(flags, &references);
        for length in 48..bytes.len() {
            assert!(
                Region::parse(&bytes[..length]).is_err(),
                "flags {flags:#x}, length {length} unexpectedly succeeded"
            );
        }
        assert!(Region::parse(&bytes).is_ok());
    }
}

#[test]
fn optional_prefixes_and_userdata_precede_both_references() {
    // Sphere, reverb volume, reverb offset, USERDATA size and bytes, then references.
    let mut bytes = minimal_region(0x147, &[]);
    bytes.truncate(44);
    for value in [1.0_f32, -2.0, 3.0, 4.0, 0.5] {
        bytes.extend(value.to_le_bytes());
    }
    bytes.extend((-7_i32).to_le_bytes());
    bytes.extend(4_u32.to_le_bytes());
    bytes.extend([0x31, 0x32, 0x33, 0x00]);
    bytes.extend(111_i32.to_le_bytes());
    bytes.extend(222_i32.to_le_bytes());

    let (remaining, region) = Region::parse(&bytes).unwrap();
    assert!(remaining.is_empty());
    assert_eq!(region.sphere, Some((1.0, -2.0, 3.0, 4.0)));
    assert_eq!(region.reverb_volume, Some(0.5));
    assert_eq!(region.reverb_offset, Some(-7));
    assert_eq!(region.reference_0x40, Some(FragmentRef::new(111)));
    assert!(matches!(
        region.mesh_reference,
        Some(FragmentRef::Index(222, _))
    ));
    assert_eq!(region.to_bytes(), bytes);
}

#[test]
fn alignment_padding_remains_distinct_from_reference_words() {
    for (userdata, padding) in [
        (&b"a"[..], &b"\0\0\0"[..]),
        (&b"ab"[..], &b"\0\0"[..]),
        (&b"abc"[..], &b"\0"[..]),
    ] {
        let mut bytes = minimal_region(0x140, &[]);
        bytes.truncate(44);
        bytes.extend((userdata.len() as u32).to_le_bytes());
        bytes.extend(userdata);
        bytes.extend(111_i32.to_le_bytes());
        bytes.extend(222_i32.to_le_bytes());
        bytes.extend(padding);
        let (remaining, region) = Region::parse(&bytes).unwrap();
        assert_eq!(remaining, padding);
        assert_eq!(region.reference_0x40, Some(FragmentRef::new(111)));
        assert!(matches!(
            region.mesh_reference,
            Some(FragmentRef::Index(222, _))
        ));
        assert_eq!(region.to_bytes(), bytes);
        let serialized = region.to_bytes();
        let (remaining, reparsed) = Region::parse(&serialized).unwrap();
        assert_eq!(remaining, padding);
        assert_eq!(reparsed, region);
    }
}

#[test]
fn absent_reference_flags_leave_extra_words_unconsumed() {
    let bytes = minimal_region(0, &[111, 222]);
    let (remaining, region) = Region::parse(&bytes).unwrap();
    assert_eq!(remaining, &bytes[48..]);
    assert!(region.reference_0x40.is_none());
    assert!(region.mesh_reference.is_none());
    assert_eq!(region.to_bytes(), &bytes[..48]);
}

#[test]
fn complete_document_keeps_the_modern_reference_in_the_typed_region() {
    let region_bytes = minimal_region(0x140, &[111, 222]);
    // WLD header followed by one Region fragment; references intentionally unresolved.
    let header: [u32; 9] = [0x5450_3d02, 0x0001_5500, 1, 1, 56, 0, 0, 56, 0x22];
    let mut bytes: Vec<u8> = header.into_iter().flat_map(u32::to_le_bytes).collect();
    bytes.extend(region_bytes);
    let document = WldDoc::parse(&bytes).unwrap();
    let regions: Vec<_> = document.fragment_iter::<Region>().collect();
    assert_eq!(regions.len(), 1);
    assert_eq!(regions[0].reference_0x40, Some(FragmentRef::new(111)));
    assert!(matches!(
        regions[0].mesh_reference,
        Some(FragmentRef::Index(222, _))
    ));
}
