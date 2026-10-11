#![cfg(feature = "nbt")]

use nbtx::{Value, from_be_bytes};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct PeakLive;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn grew(size: usize) {
    let live = LIVE.fetch_add(size, Ordering::Relaxed) + size;
    PEAK.fetch_max(live, Ordering::Relaxed);
}

unsafe impl GlobalAlloc for PeakLive {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        grew(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        grew(new_size);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: PeakLive = PeakLive;

#[test]
fn nested_lists_with_huge_counts_reserve_little_memory() {
    let mut bytes = vec![10, 0, 0, 9, 0, 0];
    for _ in 0..300 {
        bytes.push(9);
        bytes.extend_from_slice(&0x0fff_ffffu32.to_be_bytes());
    }

    let before = LIVE.load(Ordering::Relaxed);
    PEAK.store(before, Ordering::Relaxed);
    let result: Result<Value, _> = from_be_bytes(&mut bytes.as_slice());
    let peak = PEAK.load(Ordering::Relaxed) - before;

    assert!(result.is_err());
    assert!(
        peak < 4 << 20,
        "{} input bytes peaked at {peak} bytes",
        bytes.len()
    );
}
