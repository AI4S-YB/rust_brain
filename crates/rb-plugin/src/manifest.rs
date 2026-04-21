//! Manifest data types — filled in Task 2.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct PluginManifest;
#[derive(Debug, Clone, Deserialize)]
pub struct BinarySpec;
#[derive(Debug, Clone, Deserialize)]
pub struct ParamSpec;
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
pub enum ParamType { Placeholder }
#[derive(Debug, Clone, Deserialize)]
pub struct CliRule;
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OutputSpec;
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Strings;
