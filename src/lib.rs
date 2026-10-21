pub mod agent;
pub(crate) mod actions;
pub mod align;
pub mod anim_export;
pub mod app;
pub mod artwork_mask;
pub mod asset_browser;
pub mod boolean;
pub mod brand;
pub mod brushes;
pub mod clipboard;
pub mod cloud;
pub mod collaboration;
pub mod color;
pub mod channels;
pub mod compositor;
pub mod compound;
pub mod document;
pub mod deep_color;
pub mod filter;
pub mod formats;
pub mod publishing;
pub mod geom;
pub mod google_fonts;
pub mod gradient;
pub(crate) mod graphic_styles;
pub(crate) mod linked_styles;
pub(crate) mod preset_file;
pub mod import;
pub mod layout;
pub mod layout_components;
pub mod layout_demo;
pub mod layout_export;
pub mod layout_images;
pub mod layout_prototype;
pub mod layout_templates;
pub mod layout_tokens;
pub(crate) mod layer_clipping;
pub mod motion;
pub mod scene3d;
pub mod ml;
pub mod background_removal;
pub mod upscale;
pub mod export;
pub mod motion_presets;
pub mod paint;
pub mod palette;
mod phosphor_map;
pub mod photo;
pub mod plugins;
pub mod presets;
pub mod project;
pub mod raster;
pub mod release_version;
pub mod screen_pick;
pub mod selection_coverage;
pub mod screen_export;
pub mod shape_browser;
pub mod simplify;
pub mod shots;
pub mod snap;
pub mod source_attachment;
pub mod svg;
pub mod svg_source;
pub mod templates;
pub mod text;
pub mod text_geometry;
pub mod tools;
pub mod trace;
pub mod typography;
pub mod ui;

pub mod deform;

pub mod outline;

pub mod telemetry;
pub mod resources;

#[cfg(test)]
pub(crate) mod test_support {
    use std::path::PathBuf;

    /// Test scratch root
    /// Brief: Writable scratch directory for test receipts and fixtures that never lands on the root disk.
    /// Args: `segment` - unique path segment for the test family, e.g. `color-themes` or `craft-motion`.
    /// Returns: an absolute path under `TMPDIR` when that directory lives under `/home`, otherwise `.qa-runs` beside the manifest (the repo's serialized test helper guarantees TMPDIR under /home; a bare cargo run falls back to the manifest-side .qa-runs directory).
    pub(crate) fn scratch(segment: &str) -> PathBuf {
        let fallback = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".qa-runs");
        match std::env::var_os("TMPDIR").map(PathBuf::from) {
            Some(root) if root.starts_with("/home") => root.join(segment),
            _ => fallback.join(segment),
        }
    }
}
