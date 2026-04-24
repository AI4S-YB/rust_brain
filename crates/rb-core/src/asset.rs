use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum AssetKind {
    StarIndex,
    Bam,
    TrimmedFastq,
    Gtf,
    CountsMatrix,
    Report,
    Other,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AssetRecord {
    pub id: String,
    pub kind: AssetKind,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub produced_by_run_id: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Declared by `Module::produced_assets()` so the Runner knows what to
/// automatically register after a successful run. The `relative_path` is
/// resolved against the run's output directory.
#[derive(Debug, Clone)]
pub struct DeclaredAsset {
    pub kind: AssetKind,
    pub relative_path: PathBuf,
    pub display_name: String,
    pub schema: Option<String>,
}

pub fn new_asset_id() -> String {
    let short = Uuid::new_v4().to_string()[..8].to_string();
    format!("as_{}", short)
}
