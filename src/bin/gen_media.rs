fn main() {
    let status = std::process::Command::new("node")
        .arg("scripts/refresh-product-media.mjs")
        .args(std::env::args().skip(1))
        .status()
        .expect("Node is required to refresh native product media");
    std::process::exit(status.code().unwrap_or(1));
}
