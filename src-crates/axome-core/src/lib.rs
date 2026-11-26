mod dirs;
pub mod errors;
pub mod events;
mod host;
mod operations;
pub mod services;
pub mod workbench_snapshot;

pub use dirs::HostDirs;
pub use host::{Host, HostOptions};

/// Encodes raw digest bytes as lowercase hexadecimal.
pub fn encode_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

pub const SUPERVISOR_ROUTE: &str = "__axo-supervisor";

pub fn run_supervisor(
    args: impl IntoIterator<Item = std::ffi::OsString>,
) -> std::process::ExitCode {
    axo::exec::supervisor::run(args)
}
