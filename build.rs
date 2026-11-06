use std::env;
#[path = "src/release_version.rs"]
mod release_version;

fn main() {
    let cargo_version = env::var("CARGO_PKG_VERSION").expect("Cargo package version");
    assert_eq!(
        release_version::cargo_version(&release_version::public_version(&cargo_version)),
        cargo_version
    );
    println!("cargo:rerun-if-changed=src/release_version.rs");
    println!(
        "cargo:rustc-env=OMADESIGN_VERSION={}",
        release_version::public_version(&cargo_version)
    );
}
