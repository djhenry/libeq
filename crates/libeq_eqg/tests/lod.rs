use libeq_eqg::lod::{self, ParseError, RecordKind};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

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

#[test]
fn preserves_one_to_five_levels_in_written_order() {
    let rows = [
        "LOD,A,300\n",
        "LOD,B,10\n",
        "LOD,A,010\n",
        "LOD,D,-1\n",
        "LOD,E,NaN",
    ];
    let models = [b"A", b"B", b"A", b"D", b"E"];
    let numbers: [&[u8]; 5] = [b"300", b"10", b"010", b"-1", b"NaN"];
    for count in 1..=5 {
        let input = format!("EQLOD\n{}", rows[..count].concat());
        let list = lod::parse(input.as_bytes()).unwrap();
        let actual: Vec<_> = list.records().map(|row| row.kind).collect();
        let expected: Vec<_> = (0..count)
            .map(|index| RecordKind::Level {
                model: models[index],
                raw_number: Some(numbers[index]),
            })
            .collect();
        assert_eq!(actual, expected);
    }
}

#[test]
fn raw_rows_keep_offsets_names_number_presence_and_unknowns() {
    let input =
        b"EQLOD\r\nLOD,A\r\nLOD,B,\nlOd, \xff , -001,x\r\nCOL,X,Y\r\ncol,\n ignored\n\nLOD,,NaN\r";
    let list = lod::parse(input).unwrap();
    assert_eq!(list.source(), input);
    assert_eq!(list.source().as_ptr(), input.as_ptr());
    let rows: Vec<_> = list.records().collect();
    assert_eq!(
        rows.iter()
            .map(|r| (r.byte_offset, r.line_number))
            .collect::<Vec<_>>(),
        [
            (7, 2),
            (14, 3),
            (21, 4),
            (38, 5),
            (47, 6),
            (52, 7),
            (61, 8),
            (62, 9)
        ]
    );
    assert_eq!(
        rows.iter().map(|r| r.kind).collect::<Vec<_>>(),
        [
            RecordKind::Level {
                model: b"A",
                raw_number: None
            },
            RecordKind::Level {
                model: b"B",
                raw_number: Some(b"")
            },
            RecordKind::Level {
                model: b" \xff ",
                raw_number: Some(b" -001,x")
            },
            RecordKind::Collision { model: b"X,Y" },
            RecordKind::Collision { model: b"" },
            RecordKind::Unknown,
            RecordKind::Unknown,
            RecordKind::Level {
                model: b"",
                raw_number: Some(b"NaN")
            },
        ]
    );
    assert_eq!(rows[2].raw_line, b"lOd, \xff , -001,x");
    assert_eq!(rows[5].raw_line, b" ignored");
    assert_eq!(rows[6].raw_line, b"");
    for row in &rows {
        assert_eq!(row.raw_line.as_ptr(), input[row.byte_offset..].as_ptr());
    }
    let RecordKind::Level { model, raw_number } = rows[2].kind else {
        panic!()
    };
    assert_eq!(model.as_ptr(), input[25..].as_ptr());
    assert_eq!(raw_number.unwrap().as_ptr(), input[29..].as_ptr());
    assert_eq!(rows[3].kind, list.records().nth(3).unwrap().kind);
    assert_eq!(list.records().count(), 8);
}

#[test]
fn duplicate_collisions_and_raw_numeric_spellings_are_not_interpreted() {
    let input = b"EQLOD\nCOL,Model,Part\ncol,model,Part\nLOD,MiXeD,+inf\nLOD,mixed,0x10\nLOD,z,1e999\nLOD,z,\t 005 \t";
    let rows: Vec<_> = lod::parse(input).unwrap().records().collect();
    assert_eq!(rows.len(), 6);
    assert_eq!(
        rows[0].kind,
        RecordKind::Collision {
            model: b"Model,Part"
        }
    );
    assert_eq!(
        rows[1].kind,
        RecordKind::Collision {
            model: b"model,Part"
        }
    );
    for (row, expected) in
        rows[2..]
            .iter()
            .zip([b"+inf".as_slice(), b"0x10", b"1e999", b"\t 005 \t"])
    {
        let RecordKind::Level { raw_number, .. } = row.kind else {
            panic!()
        };
        assert_eq!(raw_number, Some(expected));
    }
    let RecordKind::Level { model, .. } = rows[2].kind else {
        panic!()
    };
    assert_eq!(model, b"MiXeD");
    assert!(model.eq_ignore_ascii_case(b"mixed"));
}

#[test]
fn strips_only_one_trailing_cr_and_accepts_a_final_unterminated_row() {
    let rows: Vec<_> = lod::parse(b"EQLOD\nLOD,A,1\r\r\nCOL,C\r\r")
        .unwrap()
        .records()
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].raw_line, b"LOD,A,1\r");
    assert_eq!(
        rows[0].kind,
        RecordKind::Level {
            model: b"A",
            raw_number: Some(b"1\r")
        }
    );
    assert_eq!(rows[1].raw_line, b"COL,C\r");
    assert_eq!(rows[1].kind, RecordKind::Collision { model: b"C\r" });
    assert_eq!((rows[1].byte_offset, rows[1].line_number), (16, 3));
}

#[test]
fn empty_bodies_and_physical_blank_rows_have_no_phantom_terminal_row() {
    for input in [b"EQLOD\n".as_slice(), b"EQLOD\r\n"] {
        assert_eq!(lod::parse(input).unwrap().records().count(), 0);
    }
    for input in [b"EQLOD\n\n".as_slice(), b"EQLOD\n\r\n"] {
        let rows: Vec<_> = lod::parse(input).unwrap().records().collect();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, RecordKind::Unknown);
        assert_eq!(rows[0].raw_line, b"");
        assert_eq!((rows[0].byte_offset, rows[0].line_number), (6, 2));
    }
    let rows: Vec<_> = lod::parse(b"EQLOD\nCOL,only").unwrap().records().collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].kind, RecordKind::Collision { model: b"only" });
}

#[test]
fn leading_whitespace_and_partial_tags_remain_unknown_without_trimming() {
    let input = b"EQLOD\n LOD,A,1\nLOD\nCOL\nLODx,A,1\nCOLL,A\n\rinside\n";
    let rows: Vec<_> = lod::parse(input).unwrap().records().collect();
    assert_eq!(rows.len(), 6);
    assert!(rows.iter().all(|r| r.kind == RecordKind::Unknown));
    assert_eq!(rows[0].raw_line, b" LOD,A,1");
    assert_eq!(rows[5].raw_line, b"\rinside");
}

#[test]
fn framing_errors_distinguish_magic_ending_and_original_nul_offset() {
    for input in [
        b"".as_slice(),
        b"E",
        b"EQLO",
        b"EQLOX\n",
        b"eqLOD\n",
        b" EQLOD\n",
        b"\xef\xbb\xbfEQLOD\n",
    ] {
        assert_eq!(lod::parse(input).unwrap_err(), ParseError::InvalidMagic);
    }
    for input in [
        b"EQLOD".as_slice(),
        b"EQLODx",
        b"EQLOD\r",
        b"EQLOD\rx",
        b"EQLOD\0",
    ] {
        assert_eq!(
            lod::parse(input).unwrap_err(),
            ParseError::InvalidHeaderEnding { offset: 5 }
        );
    }
    let input = b"EQLOD\nLOD,A,1\nCOL,C\n";
    for offset in 6..input.len() {
        let mut bytes = input.to_vec();
        bytes[offset] = 0;
        assert_eq!(
            lod::parse(&bytes).unwrap_err(),
            ParseError::InteriorNul { offset }
        );
    }
    assert_eq!(
        lod::parse(b"EQLOD\n\0").unwrap_err(),
        ParseError::InteriorNul { offset: 6 }
    );
    assert_eq!(lod::parse(b"BAD\0").unwrap_err(), ParseError::InvalidMagic);
}

#[test]
fn public_parse_and_record_iteration_allocate_nothing_on_the_counted_thread() {
    // Initialize the thread-local counters before enabling them; other test threads
    // are excluded. Fixture construction and assertions stay outside the interval.
    ALLOCATIONS.with(|count| count.set(0));
    COUNTING.with(|enabled| enabled.set(true));
    let control = std::hint::black_box(Box::new(7_u8));
    COUNTING.with(|enabled| enabled.set(false));
    let control_allocations = ALLOCATIONS.with(Cell::get);
    drop(control);
    assert!(
        control_allocations > 0,
        "counter must detect a real allocation"
    );
    ALLOCATIONS.with(|count| count.set(0));
    COUNTING.with(|enabled| enabled.set(true));
    let parsed = lod::parse(b"EQLOD\r\nLOD,A\nLOD,B,\nLOD,C,NaN,extra\nCOL,X,Y\n\nUnknown");
    let count = parsed.as_ref().map(|list| list.records().count());
    let source_length = parsed.as_ref().map(|list| list.source().len());
    let error = lod::parse(b"EQLOD\nLOD,A\0");
    COUNTING.with(|enabled| enabled.set(false));
    let allocations = ALLOCATIONS.with(Cell::get);
    assert_eq!(allocations, 0);
    assert_eq!(count.unwrap(), 6);
    assert_eq!(source_length.unwrap(), 52);
    assert!(matches!(error, Err(ParseError::InteriorNul { offset: 11 })));
}
