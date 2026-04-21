//! TOML-manifest-driven plugin system for external CLI tools.
//!
//! See `docs/superpowers/specs/2026-04-21-third-party-tool-plugins-design.md`.

pub mod argv;
pub mod loader;
pub mod manifest;
pub mod module;
pub mod schema;
pub mod subprocess;
pub mod validate;

pub use loader::{load_plugins, LoadedPlugin, PluginRegistry, PluginSource};
pub use manifest::{BinarySpec, CliRule, OutputSpec, ParamSpec, ParamType, PluginManifest, Strings};
pub use module::ExternalToolModule;
pub use validate::{ManifestIssue, ManifestIssueLevel};
