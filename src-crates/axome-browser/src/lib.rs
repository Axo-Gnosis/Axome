//! Axome browser-use engine backed by `playwright-rs`.
//!
//! Owns browser discovery, Playwright driver / Chromium lifecycle,
//! per-session `BrowserContext`s (isolated in local mode, shared in CDP mode),
//! named tabs, telemetry deltas, and the deferred `browser` `axo_agent::Tool`.

pub mod discovery;
pub mod policy;
pub mod service;
pub mod tab;
pub mod tool;

pub use discovery::{discover_browsers, resolve_executable, DetectedBrowser};
pub use policy::validate_navigation_url;
pub use service::{BrowserConfig, BrowserService};
