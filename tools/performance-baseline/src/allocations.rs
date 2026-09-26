use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct CountingAllocator;

static CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every operation is delegated unchanged to `System`. The counters do
// not participate in pointer ownership and use relaxed atomics only for
// diagnostic totals, so they cannot affect allocator correctness.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        CALLS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        // SAFETY: the caller supplied a valid layout and `System` is the backing allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: the pointer and layout are forwarded to the allocator that created it.
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        CALLS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(size, Ordering::Relaxed);
        // SAFETY: the pointer/layout pair and requested size are forwarded unchanged.
        unsafe { System.realloc(pointer, layout, size) }
    }
}

pub fn reset() {
    CALLS.store(0, Ordering::Relaxed);
    BYTES.store(0, Ordering::Relaxed);
}

pub fn snapshot() -> (usize, usize) {
    (CALLS.load(Ordering::Relaxed), BYTES.load(Ordering::Relaxed))
}
