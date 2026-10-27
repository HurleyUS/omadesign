use std::{env, fs, path::Path};

/// Build the vendored LibRaw release and the C++ bridge into one static archive.
/// Args: none. Returns after cc has emitted the native link instructions.
fn main() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let archive = manifest.join("../../vendor/libraw/LibRaw-0.22.2.tar.gz");
    let bridge = manifest.join("src/bridge.cpp");
    println!("cargo:rerun-if-changed={}", archive.display());
    println!("cargo:rerun-if-changed={}", bridge.display());
    println!("cargo:rerun-if-env-changed=OMA_RAW_CXX_STDLIB");
    let out = env::var_os("OUT_DIR").expect("Cargo output directory");
    let out = Path::new(&out);
    let source = out.join("LibRaw-0.22.2");
    // The complete, unchanged release archive is versioned in this repository.
    // Builds never download native code or depend on an installed RAW decoder.
    if !source.join("src/libraw_c_api.cpp").is_file() {
        tar::Archive::new(flate2::read::GzDecoder::new(
            fs::File::open(&archive).expect("vendored LibRaw source"),
        ))
        .unpack(out)
        .expect("extract vendored LibRaw");
    }
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++11")
        // Sensor demosaicing remains interactive in development builds, too.
        .opt_level(2)
        .warnings(false)
        .include(&source)
        .define("USE_JPEG", None)
        .define("USE_ZLIB", None)
        .define("USE_X3FTOOLS", None)
        .define("LIBRAW_MAX_ALLOC_MB_DEFAULT", "1024")
        .file(&bridge);
    for key in ["DEP_JPEG_INCLUDE", "DEP_Z_INCLUDE"] {
        let value = env::var_os(key).unwrap_or_else(|| panic!("missing {key}"));
        for include in env::split_paths(&value) {
            build.include(include);
        }
    }
    add_sources(&mut build, &source.join("src"));
    if let Ok(library) = env::var("OMA_RAW_CXX_STDLIB") {
        build.cpp_link_stdlib(&*library);
    }
    build.compile("omadesign_raw");
}

fn add_sources(build: &mut cc::Build, directory: &Path) {
    let mut entries = fs::read_dir(directory)
        .expect("LibRaw source directory")
        .map(|entry| entry.expect("LibRaw source entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for entry in entries {
        if entry.is_dir() {
            add_sources(build, &entry);
        } else if entry.extension().is_some_and(|ext| ext == "cpp")
            // `*_ph.cpp` are LibRaw's placeholder stubs for builds without the
            // real pre/post-processing and writer units; they duplicate symbols.
            && !entry.file_stem().is_some_and(|stem| stem.to_string_lossy().ends_with("_ph"))
        {
            build.file(entry);
        }
    }
}
