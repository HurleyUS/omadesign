//! Isolated XDG profiles for QA binaries without mutating the process
//! environment (`std::env::set_var` is `unsafe` since Rust 2024, #486).

use std::{ffi::OsStr, process::Command};

/// Marker set on the re-executed child so it never re-executes again.
pub const ISOLATED_MARKER: &str = "OMADESIGN_QA_ENV_ISOLATED";

/// Make sure the current process runs with `vars` in its environment.
///
/// When the variables are not already present, the current executable is
/// re-run with the same arguments plus `vars` (set safely through
/// [`Command::envs`]) and this process exits with the child's status.
/// The child sees [`ISOLATED_MARKER`] and returns immediately.
/// Args: variable/value pairs. Returns only when the environment is in place.
pub fn ensure_env<K: AsRef<OsStr>, V: AsRef<OsStr>>(vars: &[(K, V)]) {
    let already = vars
        .iter()
        .all(|(key, value)| std::env::var_os(key).as_deref() == Some(value.as_ref()));
    if already || std::env::var_os(ISOLATED_MARKER).is_some() {
        return;
    }
    let exe = std::env::current_exe().expect("QA executable path");
    let status = Command::new(exe)
        .args(std::env::args_os().skip(1))
        .envs(vars.iter().map(|(key, value)| (key.as_ref(), value.as_ref())))
        .env(ISOLATED_MARKER, "1")
        .status()
        .expect("re-run QA binary with an isolated environment");
    std::process::exit(status.code().unwrap_or(1));
}
