//! LibRaw 0.22.2 bridge for Omadesign.
//!
//! This crate is the only place the vendored LibRaw and its C++ adapter
//! (`bridge.cpp`) are compiled and called. Callers implement [`Host`] for
//! cancellation, progress and memory admission and get back an owned
//! [`Image`] whose native allocation is freed exactly once on drop (#486).

use std::{ffi::c_void, os::raw::c_char};

// These crates provide the statically built decompression libraries consumed by
// the C++ bridge. Referencing them retains their native link metadata.
use libz_sys as _;
use mozjpeg_sys as _;

/// Decoder output in the bridge's exact `repr(C)` layout.
/// Plain fields are public; the native owner and pixel pointer stay private
/// and are reached through [`Image::data`].
#[repr(C)]
pub struct Image {
    owner: *mut c_void,
    data: *const u8,
    pub data_size: u64,
    pub width: u32,
    pub height: u32,
    pub colors: u32,
    pub bits: u32,
    pub warnings: u32,
    pub raw_count: u32,
    pub dng_opcodes: u32,
    pub orientation: i32,
    pub iso: f32,
    pub shutter: f32,
    pub aperture: f32,
    pub focal_length: f32,
    pub camera_wb: [f32; 4],
    pub baseline_exposure: f32,
    pub timestamp: i64,
    pub make: [c_char; 64],
    pub model: [c_char; 64],
    pub lens: [c_char; 128],
    pub error: [c_char; 256],
}

// SAFETY: `Image` exclusively owns its native allocation; the bridge keeps no
// reference to it after `oma_raw_decode` returns, so moving it between
// threads is sound. Shared access only reads immutable bytes.
unsafe impl Send for Image {}
// SAFETY: see `Send`; no method mutates through `&Image`.
unsafe impl Sync for Image {}

impl Image {
    /// Borrow the decoded pixel bytes.
    /// Args: none. Returns `None` when the bridge produced no buffer or its
    /// size cannot be addressed as one slice.
    pub fn data(&self) -> Option<&[u8]> {
        if self.data.is_null() || self.data_size > isize::MAX as u64 {
            return None;
        }
        // SAFETY: the bridge allocation owns exactly `data_size` readable bytes
        // at `data` and lives until `self` is dropped; the size is bounded above.
        Some(unsafe { std::slice::from_raw_parts(self.data, self.data_size as usize) })
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        // SAFETY: The bridge returns sole ownership of this allocation, or null.
        // `Image` is never cloned or moved across the FFI boundary again.
        unsafe { oma_raw_free(self.owner) };
    }
}

/// Caller services the decoder consults while it runs.
/// The bridge may call these from its monitor thread, hence `Sync`.
pub trait Host: Sync {
    /// Returns true once the decode should stop.
    fn cancelled(&self) -> bool;
    /// Records the decoder's current stage.
    fn progress(&self, stage: u32);
    /// Admits `bytes` of native working memory. Returns false to refuse.
    fn reserve(&self, bytes: u64) -> bool;
}

unsafe extern "C" {
    fn oma_raw_decode(
        bytes: *const u8,
        length: usize,
        image: *mut Image,
        cancelled: unsafe extern "C" fn(*const c_void) -> i32,
        context: *const c_void,
        progress: unsafe extern "C" fn(*const c_void, u32),
        reserve: unsafe extern "C" fn(*const c_void, u64) -> i32,
        memory_context: *const c_void,
        memory_limit: u64,
        sensor_only: i32,
    ) -> i32;
    fn oma_raw_free(owner: *mut c_void);
}

/// Decode untouched camera bytes synchronously.
/// Args: encoded source, caller services, native memory ceiling, and whether
/// to return the single-plane sensor data instead of demosaiced RGB.
/// Returns the bridge status (0 on success) and the image it filled; on
/// failure the image's `error` text explains why.
pub fn decode(bytes: &[u8], host: &dyn Host, memory_limit: u64, sensor_only: bool) -> (i32, Image) {
    // SAFETY: Every field is a plain number, fixed byte array, or nullable raw
    // pointer. All-zero initialization is valid, including the Drop owner.
    let mut image: Image = unsafe { std::mem::zeroed() };
    let host: &&dyn Host = &host;
    let context = (host as *const &dyn Host).cast::<c_void>();
    // SAFETY: The borrowed byte buffer and `host` remain alive through this
    // synchronous decode, and the bridge joins its monitor before returning.
    // The C++ adapter validates input/layout, catches exceptions and writes only
    // within the exactly matching repr(C) output. It never retains input bytes.
    let status = unsafe {
        oma_raw_decode(
            bytes.as_ptr(),
            bytes.len(),
            &mut image,
            cancelled,
            context,
            progress,
            reserve,
            context,
            memory_limit,
            i32::from(sensor_only),
        )
    };
    (status, image)
}

unsafe extern "C" fn cancelled(context: *const c_void) -> i32 {
    // SAFETY: `decode` passes a `&&dyn Host` that outlives the native call.
    let host = unsafe { *context.cast::<&dyn Host>() };
    i32::from(host.cancelled())
}

unsafe extern "C" fn progress(context: *const c_void, stage: u32) {
    // SAFETY: `decode` passes a `&&dyn Host` that outlives the native call.
    let host = unsafe { *context.cast::<&dyn Host>() };
    host.progress(stage);
}

unsafe extern "C" fn reserve(context: *const c_void, bytes: u64) -> i32 {
    // SAFETY: `decode` passes a `&&dyn Host` that outlives the native call.
    let host = unsafe { *context.cast::<&dyn Host>() };
    i32::from(!host.reserve(bytes))
}
