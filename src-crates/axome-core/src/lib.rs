mod dirs;
pub mod errors;
pub mod events;
mod host;
mod operations;
pub mod services;
pub mod workbench_snapshot;

pub use dirs::HostDirs;
pub use host::{Host, HostOptions};

pub const SUPERVISOR_ROUTE: &str = "__axo-supervisor";

pub fn run_supervisor(
    args: impl IntoIterator<Item = std::ffi::OsString>,
) -> std::process::ExitCode {
    axo::exec::supervisor::run(args)
}
