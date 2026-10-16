mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::io::{ErrorKind, Read};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use common::{BLOCK_SIZE, Block, MEMBER, member_archive};
use libeq_pfs::Error;

struct ObservedSystem;

static ARMED: AtomicBool = AtomicBool::new(false);
static LARGEST: AtomicUsize = AtomicUsize::new(0);

fn record(size: usize) {
    if ARMED.load(Ordering::Relaxed) {
        LARGEST.fetch_max(size, Ordering::Relaxed);
    }
}

// Record request sizes without allocating; delegate each operation unchanged to System.
unsafe impl GlobalAlloc for ObservedSystem {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(size);
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: ObservedSystem = ObservedSystem;

// Keep this the only test in this integration binary to limit unrelated allocations.
#[test]
fn oversized_block_is_rejected_before_expanded_output_allocation() {
    let mut control = member_archive(&[Block::new(8192, &vec![0x61; BLOCK_SIZE])]);
    let mut oversized = member_archive(&[Block::new(8192, &vec![0x61; 1 << 20])]);
    for archive in [&mut control, &mut oversized] {
        assert_eq!(archive.filenames().unwrap(), [MEMBER]);
        assert_eq!(
            archive.info(MEMBER).unwrap().unwrap().uncompressed_size,
            8192
        );
    }

    LARGEST.store(0, Ordering::Relaxed);
    ARMED.store(true, Ordering::Relaxed);
    let control_result = control.get_reader(MEMBER);
    ARMED.store(false, Ordering::Relaxed);
    let control_largest = LARGEST.load(Ordering::Relaxed);

    let mut actual = Vec::new();
    control_result
        .unwrap()
        .unwrap()
        .read_to_end(&mut actual)
        .unwrap();
    assert_eq!(actual, vec![0x61; BLOCK_SIZE]);

    LARGEST.store(0, Ordering::Relaxed);
    ARMED.store(true, Ordering::Relaxed);
    let oversized_result = oversized.get_reader(MEMBER);
    ARMED.store(false, Ordering::Relaxed);
    let oversized_largest = LARGEST.load(Ordering::Relaxed);

    // A regression discriminator with ample decoder overhead, not an RSS budget.
    let allowance = control_largest.saturating_mul(4).max(128 * 1024);
    assert!(
        oversized_largest <= allowance,
        "expanded output allocation was not prevented: control={control_largest}, oversized={oversized_largest}, allowance={allowance}"
    );
    match oversized_result {
        Err(Error::IO(error)) => assert_eq!(error.kind(), ErrorKind::InvalidData),
        Err(other) => panic!("expected an expansion mismatch, got {other}"),
        Ok(_) => panic!("oversized block was accepted"),
    }
}
