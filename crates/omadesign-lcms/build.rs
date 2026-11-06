use std::{fs, path::Path};

/// Build the pinned color engine from the repository's vendored sources.
/// Args: none. Returns after cc has emitted the native link instructions.
fn main() {
    let vendor = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/lcms2");
    println!("cargo:rerun-if-changed={}", vendor.display());
    let mut build = cc::Build::new();
    build
        .warnings(false)
        .include(vendor.join("include"))
        .include(vendor.join("src"))
        .define("CMS_STATIC", None);
    let mut sources = fs::read_dir(vendor.join("src"))
        .expect("vendored Little CMS sources")
        .map(|e| e.expect("Little CMS source entry").path())
        .filter(|p| p.extension().is_some_and(|s| s == "c"))
        .collect::<Vec<_>>();
    sources.sort();
    for source in sources {
        build.file(source);
    }
    build.compile("omadesign_lcms2");
}
