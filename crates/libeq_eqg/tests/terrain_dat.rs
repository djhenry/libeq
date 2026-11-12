use libeq_eqg::terrain_dat::{self, Limits, PackedV20, ParseError};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::ops::Range;

struct CountingAllocator;
thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
fn count_allocation() {
    let _ = COUNTING.try_with(|enabled| {
        if enabled.get() {
            let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        }
    });
}
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count_allocation();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn word(out: &mut Vec<u8>, value: u32) {
    out.extend(value.to_le_bytes());
}
fn name(out: &mut Vec<u8>, value: &[u8]) {
    out.extend(value);
    out.push(0);
}
fn header(value: &[u8], tiles: u32) -> Vec<u8> {
    let mut out = Vec::new();
    for value in [20, 0x1122_3344, 0x5566_7788] {
        word(&mut out, value);
    }
    name(&mut out, value);
    word(&mut out, tiles);
    out
}
fn full_tile(out: &mut Vec<u8>, second: bool) {
    let seed = u8::from(second);
    let coords = if second {
        [0x8000_0001, 0x9abc_def0]
    } else {
        [0xffff_fffe, 0x1234_5678]
    };
    for value in coords {
        word(out, value);
    }
    word(out, 0x4455_6677 + u32::from(seed));
    for value in [
        0x7fc0_1234,
        0x8000_0000,
        0x7f80_0000,
        0xff80_0000,
        1,
        2,
        3,
        0x0102_0304,
        u32::MAX,
    ] {
        word(out, value ^ u32::from(seed));
    }
    for value in [0x1020_3040 + u32::from(seed), 0x8090_a0b0 + u32::from(seed)] {
        for _ in 0..9 {
            word(out, value);
        }
    }
    out.extend([0x80, seed, 0xff, 0x11]);
    word(out, 0x1111_2222 + u32::from(seed));
    word(out, 0x3333_4444 + u32::from(seed));
    word(out, 3);
    name(out, b"A");
    name(out, b"");
    word(out, 0);
    name(out, b"A");
    word(out, 2);
    out.extend([1, 2, 3, 4 + seed]);
    word(out, 1);
    name(out, b"a\xfe");
    name(out, b"");
    out.extend([0xa1 + seed; 44]);
    out.push(0xe5 + seed);
    word(out, 1);
    name(out, b"B");
    word(out, 0x0123_4567 + u32::from(seed));
    name(out, b"b");
    out.extend([0xb2 + seed; 56]);
    word(out, 1);
    name(out, b"");
    name(out, b"C");
    out.push(0xf6 + seed);
    out.extend([0xc3 + seed; 48]);
    word(out, 1);
    name(out, b"D\xff");
    out.extend([0xd4 + seed; 48]);
}
fn fixture() -> Vec<u8> {
    let mut out = header(b"H\xff", 2);
    full_tile(&mut out, false);
    full_tile(&mut out, true);
    out.extend(b"END\0\xfa");
    out
}
fn zero_section_tile(q: u32) -> Vec<u8> {
    let mut out = header(b"", 1);
    let grid = ((q + 1) * (q + 1)) as usize;
    out.resize(out.len() + 20 + 12 * grid + (q * q) as usize, 0);
    for _ in 0..5 {
        word(&mut out, 0);
    }
    out
}
fn borrowed(bytes: &[u8], source: &[u8], range: Range<usize>) {
    assert_eq!(bytes, &source[range.clone()]);
    assert_eq!(bytes.as_ptr(), source[range.start..].as_ptr());
}

#[test]
fn two_full_q2_tiles_preserve_every_range_and_wire_field() {
    let source = fixture();
    assert_eq!(source.len(), 794);
    let parsed = terrain_dat::parse_packed_v20(&source, 2).unwrap();
    assert_eq!(parsed.source().as_ptr(), source.as_ptr());
    assert_eq!(parsed.source(), source);
    assert_eq!(parsed.quads_per_tile(), 2);
    assert_eq!(parsed.consumed_len(), 789);
    borrowed(parsed.trailing_data(), &source, 789..794);
    assert_eq!(parsed.header().source_range, 0..19);
    assert_eq!(parsed.header().words, [20, 0x1122_3344, 0x5566_7788]);
    assert_eq!(parsed.header().name.byte_offset, 12);
    borrowed(parsed.header().name.bytes, &source, 12..14);
    assert_eq!(parsed.header().tile_count, 2);
    assert_eq!(parsed.tiles().len(), 2);
    for (
        index,
        (tile_range, grids, quad, layer_ranges, coverage_ranges, record_ranges, names, blobs),
    ) in [
        (
            19..404,
            [31..67, 67..103, 103..139],
            139..143,
            [155..157, 157..162, 162..172],
            [158..162, 164..172],
            [176..225, 229..293, 297..349, 353..404],
            [176, 179, 229, 235, 297, 298, 353],
            [180..224, 237..293, 301..349, 356..404],
        ),
        (
            404..789,
            [416..452, 452..488, 488..524],
            524..528,
            [540..542, 542..547, 547..557],
            [543..547, 549..557],
            [561..610, 614..678, 682..734, 738..789],
            [561, 564, 614, 620, 682, 683, 738],
            [565..609, 622..678, 686..734, 741..789],
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let tile = &parsed.tiles()[index];
        let seed = index as u8;
        assert_eq!(tile.source_range, tile_range);
        assert_eq!(
            tile.coordinates,
            if index == 0 {
                [0xffff_fffe, 0x1234_5678]
            } else {
                [0x8000_0001, 0x9abc_def0]
            }
        );
        assert_eq!(tile.identifier, 0x4455_6677 + index as u32);
        assert_eq!(
            tile.prefix_words,
            [0x1111_2222 + index as u32, 0x3333_4444 + index as u32]
        );
        for (bytes, range) in tile.grid_bytes.iter().zip(grids) {
            borrowed(bytes, &source, range);
        }
        assert_eq!(
            &tile.grid_bytes[0][..4],
            if index == 0 {
                &[0x34, 0x12, 0xc0, 0x7f]
            } else {
                &[0x35, 0x12, 0xc0, 0x7f]
            }
        );
        borrowed(tile.quad_bytes, &source, quad);
        assert_eq!(tile.quad_bytes, [0x80, seed, 0xff, 0x11]);
        assert_eq!(tile.layers.len(), 3);
        for (layer, range) in tile.layers.iter().zip(layer_ranges) {
            assert_eq!(layer.source_range, range);
        }
        assert_eq!(
            tile.layers.iter().map(|l| l.name.bytes).collect::<Vec<_>>(),
            [b"A".as_slice(), b"", b"A"]
        );
        for layer in &tile.layers {
            assert_eq!(layer.name.byte_offset, layer.source_range.start);
            borrowed(
                layer.name.bytes,
                &source,
                layer.name.byte_offset..layer.name.byte_offset + layer.name.bytes.len(),
            );
        }
        assert!(tile.layers[0].coverage.is_none());
        let empty = tile.layers[1].coverage.as_ref().unwrap();
        assert_eq!(empty.source_range, coverage_ranges[0]);
        assert_eq!(empty.resolution, 0);
        assert!(empty.bytes.is_empty());
        borrowed(
            empty.bytes,
            &source,
            coverage_ranges[0].end..coverage_ranges[0].end,
        );
        let map = tile.layers[2].coverage.as_ref().unwrap();
        assert_eq!(map.source_range, coverage_ranges[1]);
        assert_eq!(map.resolution, 2);
        assert_eq!(map.bytes, [1, 2, 3, 4 + seed]);
        borrowed(
            map.bytes,
            &source,
            coverage_ranges[1].start + 4..coverage_ranges[1].end,
        );
        assert_eq!(
            [
                tile.section_a.len(),
                tile.section_b.len(),
                tile.section_c.len(),
                tile.section_d.len()
            ],
            [1; 4]
        );
        assert_eq!(tile.section_a[0].source_range, record_ranges[0]);
        assert_eq!(tile.section_b[0].source_range, record_ranges[1]);
        assert_eq!(tile.section_c[0].source_range, record_ranges[2]);
        assert_eq!(tile.section_d[0].source_range, record_ranges[3]);
        let name_views = [
            &tile.section_a[0].name1,
            &tile.section_a[0].name2,
            &tile.section_b[0].name1,
            &tile.section_b[0].name2,
            &tile.section_c[0].name1,
            &tile.section_c[0].name2,
            &tile.section_d[0].name,
        ];
        for ((name, offset), expected) in name_views.into_iter().zip(names).zip([
            b"a\xfe".as_slice(),
            b"",
            b"B",
            b"b",
            b"",
            b"C",
            b"D\xff",
        ]) {
            assert_eq!(name.byte_offset, offset);
            assert_eq!(name.bytes, expected);
            borrowed(name.bytes, &source, offset..offset + expected.len());
        }
        for (bytes, range) in [
            tile.section_a[0].blob,
            tile.section_b[0].blob,
            tile.section_c[0].blob,
            tile.section_d[0].blob,
        ]
        .into_iter()
        .zip(blobs)
        {
            borrowed(bytes, &source, range);
        }
        assert_eq!(tile.section_a[0].blob, [0xa1 + seed; 44]);
        assert_eq!(tile.section_a[0].byte, 0xe5 + seed);
        assert_eq!(tile.section_b[0].word, 0x0123_4567 + index as u32);
        assert_eq!(tile.section_b[0].blob, [0xb2 + seed; 56]);
        assert_eq!(tile.section_c[0].byte, 0xf6 + seed);
        assert_eq!(tile.section_c[0].blob, [0xc3 + seed; 48]);
        assert_eq!(tile.section_d[0].blob, [0xd4 + seed; 48]);
    }
}

#[test]
fn zero_tiles_and_non_power_of_two_grid_have_no_invented_payload() {
    let mut source = header(b"", 0);
    source.extend(b"suffix");
    let limits = Limits {
        max_tiles: 0,
        max_records: 0,
        max_name_bytes: 1,
        max_parsed_bytes: 17,
    };
    let empty = terrain_dat::parse_packed_v20_with_limits(&source, 1024, &limits).unwrap();
    assert_eq!(empty.header().source_range, 0..17);
    assert!(empty.tiles().is_empty());
    assert_eq!(empty.trailing_data(), b"suffix");
    let source = zero_section_tile(3);
    assert_eq!(source.len(), 258);
    let parsed = terrain_dat::parse_packed_v20(&source, 3).unwrap();
    assert_eq!(parsed.tiles()[0].source_range, 17..258);
    assert_eq!(parsed.tiles()[0].grid_bytes.map(|b| b.len()), [64; 3]);
    assert_eq!(parsed.tiles()[0].quad_bytes.len(), 9);
    assert!(parsed.tiles()[0].layers.is_empty());
    assert!(parsed.tiles()[0].section_d.is_empty());
    assert!(parsed.trailing_data().is_empty());
    assert!(
        terrain_dat::parse_packed_v20_with_limits(
            &zero_section_tile(2),
            2,
            &Limits {
                max_records: 0,
                ..Limits::default()
            }
        )
        .is_ok()
    );
}

#[test]
fn every_prefix_truncation_refuses_instead_of_returning_partial_tiles() {
    let source = fixture();
    for end in 0..789 {
        assert!(
            matches!(
                terrain_dat::parse_packed_v20(&source[..end], 2),
                Err(ParseError::Truncated { .. })
            ),
            "end={end}"
        );
    }
    assert_eq!(
        terrain_dat::parse_packed_v20(&source[..789], 2)
            .unwrap()
            .consumed_len(),
        789
    );
}

#[test]
fn cumulative_budgets_accept_equality_and_refuse_one_less() {
    let source = fixture();
    let limits = Limits {
        max_tiles: 2,
        max_records: 14,
        max_name_bytes: 41,
        max_parsed_bytes: 789,
    };
    assert!(terrain_dat::parse_packed_v20_with_limits(&source, 2, &limits).is_ok());
    for (lower, resource, limit, requested) in [
        (
            Limits {
                max_tiles: 1,
                ..limits
            },
            "tiles",
            1,
            2,
        ),
        (
            Limits {
                max_records: 13,
                ..limits
            },
            "records",
            13,
            14,
        ),
        (
            Limits {
                max_name_bytes: 40,
                ..limits
            },
            "name bytes",
            40,
            41,
        ),
        (
            Limits {
                max_parsed_bytes: 788,
                ..limits
            },
            "parsed bytes",
            788,
            789,
        ),
    ] {
        assert_eq!(
            terrain_dat::parse_packed_v20_with_limits(&source, 2, &lower).unwrap_err(),
            ParseError::LimitExceeded {
                resource,
                limit,
                requested
            }
        );
    }
}

#[test]
fn name_search_respects_both_budgets_before_the_terminator() {
    let mut source = vec![0; 12];
    source[..4].copy_from_slice(&20_u32.to_le_bytes());
    source.extend([b'x'; 4096]);
    source.push(0);
    source.extend([0; 4]);
    assert_eq!(
        terrain_dat::parse_packed_v20_with_limits(
            &source,
            2,
            &Limits {
                max_name_bytes: 3,
                ..Limits::default()
            }
        )
        .unwrap_err(),
        ParseError::LimitExceeded {
            resource: "name bytes",
            limit: 3,
            requested: 4
        }
    );
    assert_eq!(
        terrain_dat::parse_packed_v20_with_limits(
            &source,
            2,
            &Limits {
                max_parsed_bytes: 14,
                ..Limits::default()
            }
        )
        .unwrap_err(),
        ParseError::LimitExceeded {
            resource: "parsed bytes",
            limit: 14,
            requested: 15
        }
    );
    assert_eq!(
        terrain_dat::parse_packed_v20(&source[..14], 2).unwrap_err(),
        ParseError::Truncated {
            context: "header name",
            offset: 12,
            needed: 3,
            remaining: 2
        }
    );
    let source = header(b"xy", 0);
    assert!(
        terrain_dat::parse_packed_v20_with_limits(
            &source,
            2,
            &Limits {
                max_name_bytes: 3,
                max_parsed_bytes: 19,
                ..Limits::default()
            }
        )
        .is_ok()
    );
    assert!(matches!(
        terrain_dat::parse_packed_v20_with_limits(
            &header(b"", 0),
            2,
            &Limits {
                max_name_bytes: 0,
                ..Limits::default()
            }
        ),
        Err(ParseError::LimitExceeded {
            resource: "name bytes",
            requested: 1,
            ..
        })
    ));
}

#[test]
fn versions_dimensions_and_extreme_counts_are_explicit_refusals() {
    let mut source = header(b"", 0);
    source[..4].copy_from_slice(&21_u32.to_le_bytes());
    assert_eq!(
        terrain_dat::parse_packed_v20(&source, 0).unwrap_err(),
        ParseError::UnsupportedVersion { version: 21 }
    );
    assert_eq!(
        terrain_dat::parse_packed_v20(&header(b"", 0), 0).unwrap_err(),
        ParseError::InvalidDimensions { quads_per_tile: 0 }
    );
    assert!(matches!(
        terrain_dat::parse_packed_v20(&header(b"", 0), u32::MAX),
        Err(ParseError::CountOverflow { .. })
    ));
    assert_eq!(
        terrain_dat::parse_packed_v20(&header(b"", u32::MAX), 2).unwrap_err(),
        ParseError::LimitExceeded {
            resource: "tiles",
            limit: 4096,
            requested: u32::MAX as usize
        }
    );
    let mut source = fixture();
    source[151..155].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        terrain_dat::parse_packed_v20(&source, 2).unwrap_err(),
        ParseError::LimitExceeded {
            resource: "records",
            limit: 262144,
            requested: u32::MAX as usize
        }
    );
    let mut source = fixture();
    source[164..168].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        terrain_dat::parse_packed_v20(&source, 2),
        Err(ParseError::CountOverflow { .. }
            | ParseError::LimitExceeded {
                resource: "parsed bytes",
                ..
            })
    ));
}

fn temporary_limits(source: &[u8]) -> PackedV20<'_> {
    terrain_dat::parse_packed_v20_with_limits(source, 2, &Limits::default()).unwrap()
}
#[test]
fn payload_borrows_outlive_descriptors_and_limits_are_not_borrowed() {
    let source = fixture();
    let (grid, blob, name) = {
        let parsed = temporary_limits(&source);
        (
            parsed.tiles()[0].grid_bytes[0],
            parsed.tiles()[1].section_d[0].blob,
            parsed.header().name.bytes,
        )
    };
    borrowed(grid, &source, 31..67);
    borrowed(blob, &source, 741..789);
    borrowed(name, &source, 12..14);
}

#[test]
fn late_final_record_rejection_allocates_nothing_before_complete_preflight() {
    let source = fixture();
    ALLOCATIONS.with(|count| count.set(0));
    COUNTING.with(|enabled| enabled.set(true));
    let control = std::hint::black_box(Box::new(7_u8));
    COUNTING.with(|enabled| enabled.set(false));
    let control_count = ALLOCATIONS.with(Cell::get);
    drop(control);
    assert!(control_count > 0);
    ALLOCATIONS.with(|count| count.set(0));
    COUNTING.with(|enabled| enabled.set(true));
    let rejected = terrain_dat::parse_packed_v20(&source[..788], 2);
    COUNTING.with(|enabled| enabled.set(false));
    let rejection_count = ALLOCATIONS.with(Cell::get);
    assert_eq!(rejection_count, 0);
    assert_eq!(
        rejected.unwrap_err(),
        ParseError::Truncated {
            context: "section D blob",
            offset: 741,
            needed: 48,
            remaining: 47
        }
    );
    ALLOCATIONS.with(|count| count.set(0));
    COUNTING.with(|enabled| enabled.set(true));
    let success = terrain_dat::parse_packed_v20(&source, 2);
    COUNTING.with(|enabled| enabled.set(false));
    let success_count = ALLOCATIONS.with(Cell::get);
    assert!(success.is_ok());
    assert!(
        success_count > 0,
        "indexed descriptors are allocated on success"
    );
    println!(
        "allocation control={control_count}, late rejection={rejection_count}, successful descriptors={success_count}"
    );
}
