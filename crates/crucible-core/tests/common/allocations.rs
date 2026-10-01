//! Allocation budgets for synchronous work on the calling test thread.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static BYTES: Cell<Option<usize>> = const { Cell::new(None) };
}

pub struct CountingAllocator;

fn record(bytes: usize) {
    let _ = BYTES.try_with(|count| {
        if let Some(total) = count.get() {
            count.set(Some(total.saturating_add(bytes)));
        }
    });
}

// SAFETY: Every operation forwards the original pointer/layout to System.
unsafe impl GlobalAlloc for CountingAllocator {
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

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record(new_size);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

pub fn allocated_bytes(work: impl FnOnce()) -> usize {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            BYTES.with(|count| count.set(None));
        }
    }

    BYTES.with(|count| {
        assert!(count.get().is_none(), "allocation measurements cannot nest");
        count.set(Some(0));
    });
    let _reset = Reset;
    work();
    BYTES.with(|count| count.get().unwrap())
}

/// Resident memory is diagnostic only: allocator retention varies by platform.
pub fn resident_bytes() -> Option<usize> {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        let rss = status.lines().find(|line| line.starts_with("VmRSS:"))?;
        Some(rss.split_whitespace().nth(1)?.parse::<usize>().ok()? * 1024)
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}
