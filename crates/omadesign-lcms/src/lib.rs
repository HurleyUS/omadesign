//! Little CMS 2 bridge for Omadesign.
//!
//! This crate is the only place the vendored Little CMS is compiled and
//! called. It owns profile and transform handles through RAII types so every
//! C call is one small, documented `unsafe` block (#486).

use std::ffi::c_void;

type Handle = *mut c_void;
unsafe extern "C" {
    fn cmsOpenProfileFromMem(bytes: *const c_void, size: u32) -> Handle;
    fn cmsCreate_sRGBProfile() -> Handle;
    fn cmsCloseProfile(profile: Handle) -> i32;
    fn cmsCreateTransform(
        src: Handle,
        input: u32,
        dst: Handle,
        output: u32,
        intent: u32,
        flags: u32,
    ) -> Handle;
    fn cmsDoTransform(transform: Handle, input: *const c_void, output: *mut c_void, count: u32);
    fn cmsDeleteTransform(transform: Handle);
    fn cmsGetEncodedCMMversion() -> i32;
}

/// Owned Little CMS profile handle, closed exactly once on drop.
pub struct Profile(Handle);

impl Profile {
    /// Parse an ICC profile. Returns `None` when Little CMS rejects it.
    pub fn from_icc(bytes: &[u8]) -> Option<Self> {
        let size = u32::try_from(bytes.len()).ok()?;
        // SAFETY: `bytes` is a live slice of `size` bytes for the whole call;
        // Little CMS copies what it needs and never retains the pointer.
        let handle = unsafe { cmsOpenProfileFromMem(bytes.as_ptr().cast(), size) };
        (!handle.is_null()).then_some(Self(handle))
    }

    /// Build the built-in sRGB profile. Returns `None` on allocation failure.
    pub fn srgb() -> Option<Self> {
        // SAFETY: takes no arguments and returns a new owned handle or null.
        let handle = unsafe { cmsCreate_sRGBProfile() };
        (!handle.is_null()).then_some(Self(handle))
    }
}

impl Drop for Profile {
    fn drop(&mut self) {
        // SAFETY: `self.0` is a non-null handle owned only by this value and
        // not yet closed; transforms built from it own their own copies.
        unsafe { cmsCloseProfile(self.0) };
    }
}

/// Owned Little CMS transform handle, deleted exactly once on drop.
pub struct Transform {
    handle: Handle,
    input: u32,
    output: u32,
}

impl Transform {
    /// Create a transform between two live profiles. Returns `None` when the
    /// profiles cannot provide it.
    pub fn new(src: &Profile, input: u32, dst: &Profile, output: u32, intent: u32, flags: u32) -> Option<Self> {
        // SAFETY: both profile handles are live for the call (borrowed), and
        // the format words describe float buffers that `apply` checks.
        let handle = unsafe { cmsCreateTransform(src.0, input, dst.0, output, intent, flags) };
        (!handle.is_null()).then_some(Self { handle, input, output })
    }

    /// Convert whole pixels from `source` into `target`.
    /// Args: interleaved float buffers and their per-pixel strides, which must
    /// match the formats this transform was created with.
    pub fn apply(&self, source: &[f32], input_stride: usize, target: &mut [f32], output_stride: usize) {
        let channels = |format: u32| ((format >> 3) & 15) as usize + ((format >> 7) & 1) as usize;
        assert_eq!(channels(self.input), input_stride, "ICC input stride");
        assert_eq!(channels(self.output), output_stride, "ICC output stride");
        let pixels = source.len() / input_stride;
        assert!(target.len() >= pixels * output_stride, "ICC output buffer too small");
        let count = u32::try_from(pixels).expect("ICC chunk pixel count");
        // SAFETY: the handle is live; `source` holds `pixels` whole input
        // pixels and `target` room for `pixels` output pixels in the float
        // formats asserted above, and the two slices cannot overlap.
        unsafe { cmsDoTransform(self.handle, source.as_ptr().cast(), target.as_mut_ptr().cast(), count) };
    }
}

impl Drop for Transform {
    fn drop(&mut self) {
        // SAFETY: `handle` is non-null, owned only by this value and not yet deleted.
        unsafe { cmsDeleteTransform(self.handle) };
    }
}

/// Read the linked engine's encoded version (2190 for the pinned 2.19).
/// Args: none. Returns the value Little CMS reports.
pub fn encoded_version() -> i32 {
    // SAFETY: a pure version query with no arguments or shared state.
    unsafe { cmsGetEncodedCMMversion() }
}
