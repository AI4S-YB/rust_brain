use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub default_output_dir: Option<PathBuf>,
    pub default_threads: u32,
    pub temp_dir: Option<PathBuf>,
    pub reference_genome_dir: Option<PathBuf>,
    pub annotation_file: Option<PathBuf>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            default_output_dir: None,
            default_threads: 4,
            temp_dir: None,
            reference_genome_dir: None,
            annotation_file: None,
        }
    }
}
