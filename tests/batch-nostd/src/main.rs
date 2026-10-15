//! Registers nami's `no_std` batch-state hook and runs a batched write,
//! so that linking this firmware image proves `__nami_batch_state_1_0`
//! resolves. Built for `thumbv7em-none-eabihf` by CI's embedded job;
//! excluded from the workspace so host test runs never see it. Building
//! it with `--no-default-features` leaves the hook unregistered and the
//! link fails with an undefined `__nami_batch_state_1_0`.

#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[cfg(feature = "register-hook")]
mod hook {
    use core::cell::UnsafeCell;

    /// The batch state of the image's only execution context.
    struct Shared(UnsafeCell<nami::BatchState>);

    // SAFETY: a single-core image with no interrupt handler that uses nami,
    // so no other execution context touches the state — the hook's contract.
    unsafe impl Sync for Shared {}

    static STATE: Shared = Shared(UnsafeCell::new(nami::BatchState::new()));

    fn batch_state() -> *mut nami::BatchState {
        STATE.0.get()
    }

    nami::batch_state!(batch_state);
}

/// The image entry point: the link keeps it, and through it `nami::batch`,
/// a deferred container write and the hook reference they make.
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let value = nami::binding(0_u32);
    nami::batch(|| value.set(1_u32));
    loop {
        core::hint::spin_loop();
    }
}

/// `alloc`'s symbols must resolve at link time; nothing here ever runs.
struct NullAllocator;

// SAFETY: `alloc` reports exhaustion for every request and `dealloc` is
// only ever handed pointers `alloc` returned, of which there are none.
unsafe impl core::alloc::GlobalAlloc for NullAllocator {
    unsafe fn alloc(&self, _layout: core::alloc::Layout) -> *mut u8 {
        core::ptr::null_mut()
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: core::alloc::Layout) {}
}

#[global_allocator]
static ALLOCATOR: NullAllocator = NullAllocator;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
