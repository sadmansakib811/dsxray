//! dsxray in the browser, with a plain C-style interface instead of generated JavaScript glue:
//!
//! 1. `dsx_alloc(n)` reserves `n` bytes inside the module's memory and returns where.
//! 2. JavaScript copies the file there and calls `dsx_inspect(ptr, n)`.
//! 3. It returns the address and length of a UTF-8 JSON answer, packed into one 64-bit number
//!    (`address << 32 | length`), which JavaScript reads and then hands back to `dsx_free`.
//!
//! The file never leaves the browser.

use std::slice;

/// Reserve `len` bytes for the caller to fill.
#[unsafe(no_mangle)]
pub extern "C" fn dsx_alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(len);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// Release memory from `dsx_alloc` (`cap` = the same `len`) or from `dsx_inspect`.
///
/// # Safety
/// `ptr` and `cap` must come from one `dsx_alloc` call or one `dsx_inspect` answer, freed once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dsx_free(ptr: *mut u8, cap: usize) {
    drop(unsafe { Vec::from_raw_parts(ptr, 0, cap) });
}

/// Inspect `len` bytes at `ptr`; returns `address << 32 | length` of the JSON answer.
///
/// # Safety
/// `ptr` must point at `len` initialized bytes (from `dsx_alloc`, filled by the caller).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dsx_inspect(ptr: *const u8, len: usize) -> u64 {
    let data = unsafe { slice::from_raw_parts(ptr, len) };
    let answer = dsxray::inspect_json(data).into_bytes().into_boxed_slice();
    let len = answer.len();
    let ptr = Box::into_raw(answer) as *mut u8;
    ((ptr as usize as u64) << 32) | len as u64
}
