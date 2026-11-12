use libeq_eqg::{
    FormatHeader, identify,
    terrain_project::{self, ParseError, Token},
};
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
fn requires_exact_first_token_at_byte_zero_without_changing_identification() {
    for source in [
        b"".as_slice(),
        b"E",
        b"EQTZ",
        b"EQTZ\0P",
        b"\0EQTZP",
        b"eqtzp",
        b" EQTZP",
        b"\tEQTZP",
        b"\r\nEQTZP",
        b"\xef\xbb\xbfEQTZP",
    ] {
        assert_eq!(
            terrain_project::parse(source).unwrap_err(),
            ParseError::InvalidMagic
        );
    }
    for source in [
        b"EQTZPmore".as_slice(),
        b"EQTZP\rmore",
        b"EQTZP\x0bmore\0",
        b"EQTZP\xff",
    ] {
        assert_eq!(identify(source), Ok(Some(FormatHeader::TerrainProject)));
        assert_eq!(
            terrain_project::parse(source).unwrap_err(),
            ParseError::InvalidHeaderToken { offset: 5 }
        );
    }
    for source in [
        b"EQTZP".as_slice(),
        b"EQTZP\r",
        b"EQTZP\r\n",
        b"EQTZP\x0b\x0c\r",
        b"EQTZP\t \n\x0b\x0c\r",
    ] {
        let project = terrain_project::parse(source).unwrap();
        assert_eq!(
            project.tokens().map(|t| t.bytes).collect::<Vec<_>>(),
            [b"EQTZP"]
        );
        assert_eq!(project.source(), source);
        assert_eq!(project.nul_offset(), None);
        assert_eq!(project.trailing_data(), None);
    }
    assert_eq!(
        terrain_project::parse(b"EQTZP *VERSION 12")
            .unwrap()
            .tokens()
            .count(),
        3
    );
}

#[test]
fn asymmetric_whitespace_preserves_literal_offsets_and_raw_spans() {
    let source = b"EQTZP\r\n\x0b *A\rB \x0cC\x0c\tD\x0bE\x0b\n\xff\x85=,;\"#//";
    let project = terrain_project::parse(source).unwrap();
    let tokens: Vec<_> = project.tokens().collect();
    assert_eq!(
        tokens,
        [
            Token {
                byte_offset: 0,
                raw: b"EQTZP\r",
                bytes: b"EQTZP"
            },
            Token {
                byte_offset: 9,
                raw: b"*A\rB",
                bytes: b"*A\rB"
            },
            Token {
                byte_offset: 15,
                raw: b"C\x0c",
                bytes: b"C"
            },
            Token {
                byte_offset: 18,
                raw: b"D\x0bE\x0b",
                bytes: b"D\x0bE"
            },
            Token {
                byte_offset: 23,
                raw: b"\xff\x85=,;\"#//",
                bytes: b"\xff\x85=,;\"#//"
            },
        ]
    );
    assert_eq!(project.source().as_ptr(), source.as_ptr());
    for token in tokens {
        assert_eq!(token.raw.as_ptr(), source[token.byte_offset..].as_ptr());
        assert_eq!(token.bytes.as_ptr(), token.raw.as_ptr());
    }
}

#[test]
fn first_nul_stops_tokens_but_preserves_the_complete_opaque_suffix() {
    let source = b"EQTZP *KEY value\0tail\0later";
    let project = terrain_project::parse(source).unwrap();
    assert_eq!(project.nul_offset(), Some(16));
    assert_eq!(project.trailing_data(), Some(b"tail\0later".as_slice()));
    assert_eq!(
        project.trailing_data().unwrap().as_ptr(),
        source[17..].as_ptr()
    );
    assert_eq!(project.source().as_ptr(), source.as_ptr());
    assert_eq!(project.source(), source);
    assert_eq!(
        project.tokens().collect::<Vec<_>>(),
        [
            Token {
                byte_offset: 0,
                raw: b"EQTZP",
                bytes: b"EQTZP"
            },
            Token {
                byte_offset: 6,
                raw: b"*KEY",
                bytes: b"*KEY"
            },
            Token {
                byte_offset: 11,
                raw: b"value",
                bytes: b"value"
            },
        ]
    );
    for (source, offset, suffix) in [
        (b"EQTZP\0".as_slice(), 5, b"".as_slice()),
        (b"EQTZP\0\0".as_slice(), 5, b"\0".as_slice()),
        (b"EQTZP\r\0after".as_slice(), 6, b"after".as_slice()),
        (b"EQTZP \t\0\xff".as_slice(), 7, b"\xff".as_slice()),
    ] {
        let project = terrain_project::parse(source).unwrap();
        assert_eq!(project.nul_offset(), Some(offset));
        assert_eq!(project.trailing_data(), Some(suffix));
        assert_eq!(project.tokens().count(), 1);
    }
    let project = terrain_project::parse(b"EQTZP z\r\0ignored").unwrap();
    assert_eq!(project.nul_offset(), Some(8));
    assert_eq!(
        project.tokens().nth(1),
        Some(Token {
            byte_offset: 6,
            raw: b"z\r",
            bytes: b"z"
        })
    );
}

#[test]
fn directives_quotes_punctuation_and_duplicate_keys_are_uninterpreted_tokens() {
    let source = b"EQTZP *VERSION 999 *NAME \"one two\" # *ODD =,; *VERSION -7 *NAME \xff";
    let project = terrain_project::parse(source).unwrap();
    assert_eq!(
        project.tokens().map(|t| t.bytes).collect::<Vec<_>>(),
        [
            b"EQTZP".as_slice(),
            b"*VERSION",
            b"999",
            b"*NAME",
            b"\"one",
            b"two\"",
            b"#",
            b"*ODD",
            b"=,;",
            b"*VERSION",
            b"-7",
            b"*NAME",
            b"\xff",
        ]
    );
    assert_eq!(project.tokens().count(), 13);
}

#[test]
fn opaque_long_tokens_are_borrowed_in_full_without_truncation() {
    let mut source = b"EQTZP ".to_vec();
    source.resize(6 + 256 * 1024, b'x');
    source[101] = 0xff;
    source[102] = b'\r';
    source[103] = 0x0c;
    source.extend_from_slice(b"\r\x0b");
    let project = terrain_project::parse(&source).unwrap();
    let token = project.tokens().nth(1).unwrap();
    assert_eq!(token.byte_offset, 6);
    assert_eq!(token.raw, &source[6..]);
    assert_eq!(token.bytes, &source[6..6 + 256 * 1024]);
    assert_eq!(token.bytes.len(), 256 * 1024);
    assert_eq!(token.raw.as_ptr(), source[6..].as_ptr());
    assert_eq!(project.tokens().count(), 2);
}

fn iterator_from_temporary(source: &[u8]) -> impl Iterator<Item = Token<'_>> {
    terrain_project::parse(source).unwrap().tokens()
}
#[test]
fn borrowed_views_and_iterator_outlive_the_project_handle() {
    let source = b"EQTZP *NAME preserved\0suffix";
    let (original, suffix, token) = {
        let project = terrain_project::parse(source).unwrap();
        (
            project.source(),
            project.trailing_data().unwrap(),
            project.tokens().nth(2).unwrap(),
        )
    };
    assert_eq!(original.as_ptr(), source.as_ptr());
    assert_eq!(suffix, b"suffix");
    assert_eq!(token.bytes, b"preserved");
    assert_eq!(iterator_from_temporary(source).count(), 3);
    let mut first = terrain_project::parse(source).unwrap().tokens();
    let second = terrain_project::parse(source).unwrap().tokens();
    assert_eq!(first.next().unwrap().bytes, b"EQTZP");
    assert_eq!(second.count(), 3);
    assert_eq!(first.count(), 2);
}

#[test]
fn parse_and_token_pass_allocate_nothing_with_a_live_counter_control() {
    ALLOCATIONS.with(|count| count.set(0));
    COUNTING.with(|enabled| enabled.set(true));
    let control = std::hint::black_box(Box::new(5_u8));
    COUNTING.with(|enabled| enabled.set(false));
    let control_count = ALLOCATIONS.with(Cell::get);
    drop(control);
    assert!(control_count > 0, "counter must observe a real allocation");

    ALLOCATIONS.with(|count| count.set(0));
    COUNTING.with(|enabled| enabled.set(true));
    let parsed = terrain_project::parse(b"EQTZP\r\n\x0b*A\rB z\x0c\0suffix");
    let count = parsed.as_ref().map(|p| p.tokens().count());
    let suffix = parsed.as_ref().map(|p| p.trailing_data());
    let error = terrain_project::parse(b"EQTZPx\0");
    COUNTING.with(|enabled| enabled.set(false));
    let allocations = ALLOCATIONS.with(Cell::get);
    println!("allocation control={control_count}, parse/iteration={allocations}");
    assert_eq!(allocations, 0);
    assert_eq!(count.unwrap(), 3);
    assert_eq!(suffix.unwrap(), Some(b"suffix".as_slice()));
    assert_eq!(
        error.unwrap_err(),
        ParseError::InvalidHeaderToken { offset: 5 }
    );
}
