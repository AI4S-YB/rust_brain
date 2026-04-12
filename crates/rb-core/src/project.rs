use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::module::ModuleResult;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum RunStatus {
    Pending,
    Running,
    Done,
    Failed,
    Cancelled,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RunRecord {
    pub id: String,
    pub module_id: String,
    pub params: serde_json::Value,
    pub status: RunStatus,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub result: Option<ModuleResult>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Project {
    pub name: String,
    pub created_at: DateTime<Utc>,
    #[serde(skip)]
    pub root_dir: PathBuf,
    pub runs: Vec<RunRecord>,
}

impl Project {
    pub fn create(name: &str, root_dir: &Path) -> Result<Self, io::Error> {
        fs::create_dir_all(root_dir)?;
        fs::create_dir_all(root_dir.join("input"))?;
        fs::create_dir_all(root_dir.join("runs"))?;

        let project = Project {
            name: name.to_string(),
            created_at: Utc::now(),
            root_dir: root_dir.to_path_buf(),
            runs: Vec::new(),
        };

        project.save()?;
        Ok(project)
    }

    pub fn load(root_dir: &Path) -> Result<Self, io::Error> {
        let json_path = root_dir.join("project.json");
        let data = fs::read_to_string(&json_path)?;
        let mut project: Project = serde_json::from_str(&data)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        project.root_dir = root_dir.to_path_buf();
        Ok(project)
    }

    pub fn save(&self) -> Result<(), io::Error> {
        let json_path = self.root_dir.join("project.json");
        let data = serde_json::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(&json_path, data)?;
        Ok(())
    }

    pub fn create_run(&mut self, module_id: &str, params: Value) -> RunRecord {
        let short_uuid = Uuid::new_v4().to_string()[..8].to_string();
        let run_id = format!("{}_{}", module_id, short_uuid);

        let run_dir = self.root_dir.join("runs").join(&run_id);
        fs::create_dir_all(&run_dir).expect("failed to create run directory");

        let params_path = run_dir.join("params.json");
        let params_json =
            serde_json::to_string_pretty(&params).expect("failed to serialize params");
        fs::write(&params_path, params_json).expect("failed to write params.json");

        let record = RunRecord {
            id: run_id,
            module_id: module_id.to_string(),
            params,
            status: RunStatus::Pending,
            started_at: None,
            finished_at: None,
            result: None,
        };

        self.runs.push(record.clone());
        record
    }

    pub fn run_dir(&self, run_id: &str) -> Option<PathBuf> {
        let dir = self.root_dir.join("runs").join(run_id);
        if dir.is_dir() {
            Some(dir)
        } else {
            None
        }
    }
}
