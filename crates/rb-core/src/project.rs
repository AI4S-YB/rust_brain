use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::asset::{new_asset_id, AssetKind, AssetRecord, DeclaredAsset};
use crate::input::{
    detect_kind, new_input_id, InputKind, InputPatch, InputRecord, InputScanReport,
};
use crate::module::ModuleResult;
use crate::sample::{new_sample_id, SamplePatch, SampleRecord};

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Registered InputRecord ids consumed by this run (populated by the
    /// caller via `params.inputs_used`, or left empty if not yet wired).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs_used: Vec<String>,
    /// AssetRecord ids consumed by this run (e.g. a STAR index reused from
    /// an earlier run). Empty when the run did not draw from the registry.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets_used: Vec<String>,
    /// AssetRecord ids auto-registered by the Runner from `Module::produced_assets()`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets_produced: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Project {
    pub name: String,
    pub created_at: DateTime<Utc>,
    #[serde(skip)]
    pub root_dir: PathBuf,
    pub runs: Vec<RunRecord>,
    #[serde(default = "default_view_manual")]
    pub default_view: Option<String>,
    #[serde(default)]
    pub inputs: Vec<InputRecord>,
    #[serde(default)]
    pub samples: Vec<SampleRecord>,
    #[serde(default)]
    pub assets: Vec<AssetRecord>,
}

fn default_view_manual() -> Option<String> {
    Some("manual".to_string())
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
            default_view: Some("manual".to_string()),
            inputs: Vec::new(),
            samples: Vec::new(),
            assets: Vec::new(),
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
            error: None,
            inputs_used: Vec::new(),
            assets_used: Vec::new(),
            assets_produced: Vec::new(),
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

    /// Return ids of assets produced *only* by the given run (i.e. they
    /// would become orphans if the run is deleted). Used by the UI to
    /// warn / offer cascading cleanup.
    pub fn orphan_assets_if_run_deleted(&self, run_id: &str) -> Vec<String> {
        self.assets
            .iter()
            .filter(|a| a.produced_by_run_id == run_id)
            .filter(|a| {
                // Still orphan only if no *other* run lists it as consumed.
                !self
                    .runs
                    .iter()
                    .any(|r| r.id != run_id && r.assets_used.iter().any(|x| x == &a.id))
            })
            .map(|a| a.id.clone())
            .collect()
    }

    /// Delete a run record and remove its on-disk run directory.
    /// Refuses to delete runs in `Running` or `Pending` state — caller must
    /// cancel first to avoid the Runner writing into a deleted path.
    pub fn delete_run(&mut self, run_id: &str) -> Result<(), io::Error> {
        let idx = self
            .runs
            .iter()
            .position(|r| r.id == run_id)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("run '{}' not found", run_id),
                )
            })?;

        if matches!(
            self.runs[idx].status,
            RunStatus::Running | RunStatus::Pending
        ) {
            return Err(io::Error::other(format!(
                "cannot delete run '{}' while it is {:?}; cancel it first",
                run_id, self.runs[idx].status
            )));
        }

        let dir = self.root_dir.join("runs").join(run_id);
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        self.runs.remove(idx);
        self.save()?;
        Ok(())
    }

    /// Compute the total byte size of a run directory on disk.
    /// Returns 0 if the directory does not exist.
    pub fn run_dir_size(&self, run_id: &str) -> u64 {
        let dir = self.root_dir.join("runs").join(run_id);
        dir_size_bytes(&dir).unwrap_or(0)
    }

    /// Register an external file as an Input. Idempotent on duplicate paths
    /// (canonical form): if the path is already registered, returns the
    /// existing record without creating a duplicate.
    ///
    /// Does NOT copy or move the file — we track the absolute path only.
    pub fn register_input(
        &mut self,
        path: &Path,
        kind: Option<InputKind>,
        display_name: Option<String>,
    ) -> io::Result<InputRecord> {
        let canonical = fs::canonicalize(path).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!("cannot register '{}': {}", path.display(), e),
            )
        })?;

        if let Some(existing) = self.inputs.iter().find(|r| r.path == canonical) {
            return Ok(existing.clone());
        }

        let meta = fs::metadata(&canonical)?;
        let size_bytes = meta.len();

        let display = display_name.unwrap_or_else(|| {
            canonical
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("input")
                .to_string()
        });

        let record = InputRecord {
            id: new_input_id(),
            path: canonical.clone(),
            display_name: display,
            kind: kind.unwrap_or_else(|| detect_kind(&canonical)),
            size_bytes,
            registered_at: Utc::now(),
            sample_id: None,
            paired_with: None,
            missing: false,
            notes: None,
        };

        self.inputs.push(record.clone());
        self.save()?;
        Ok(record)
    }

    /// Batch variant: best-effort registration of many files. Returns the
    /// registered records and any per-path errors. Saves once at the end.
    pub fn register_inputs_batch(
        &mut self,
        paths: &[PathBuf],
    ) -> (Vec<InputRecord>, Vec<(PathBuf, String)>) {
        let mut ok = Vec::new();
        let mut errors = Vec::new();
        let mut any_added = false;

        for p in paths {
            match self.register_input_inner(p) {
                Ok((rec, was_new)) => {
                    if was_new {
                        any_added = true;
                    }
                    ok.push(rec);
                }
                Err(e) => errors.push((p.clone(), e.to_string())),
            }
        }

        // Single save per batch.
        if any_added {
            if let Err(e) = self.save() {
                errors.push((PathBuf::from("<save>"), e.to_string()));
            }
        }
        (ok, errors)
    }

    fn register_input_inner(&mut self, path: &Path) -> io::Result<(InputRecord, bool)> {
        let canonical = fs::canonicalize(path).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!("cannot register '{}': {}", path.display(), e),
            )
        })?;
        if let Some(existing) = self.inputs.iter().find(|r| r.path == canonical) {
            return Ok((existing.clone(), false));
        }
        let meta = fs::metadata(&canonical)?;
        let rec = InputRecord {
            id: new_input_id(),
            kind: detect_kind(&canonical),
            display_name: canonical
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("input")
                .to_string(),
            path: canonical,
            size_bytes: meta.len(),
            registered_at: Utc::now(),
            sample_id: None,
            paired_with: None,
            missing: false,
            notes: None,
        };
        self.inputs.push(rec.clone());
        Ok((rec, true))
    }

    pub fn update_input(&mut self, id: &str, patch: InputPatch) -> io::Result<InputRecord> {
        let rec = self.inputs.iter_mut().find(|r| r.id == id).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, format!("input '{}' not found", id))
        })?;
        if let Some(name) = patch.display_name {
            rec.display_name = name;
        }
        if let Some(kind) = patch.kind {
            rec.kind = kind;
        }
        if let Some(notes) = patch.notes {
            rec.notes = if notes.is_empty() { None } else { Some(notes) };
        }
        let out = rec.clone();
        self.save()?;
        Ok(out)
    }

    /// Remove an input registration. Does NOT delete the file on disk —
    /// only the project's record of it.
    ///
    /// Refuses if the input is referenced by any Sample; callers must
    /// unlink or delete the referencing samples first.
    pub fn delete_input(&mut self, id: &str) -> io::Result<()> {
        let idx = self.inputs.iter().position(|r| r.id == id).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, format!("input '{}' not found", id))
        })?;

        let refs: Vec<String> = self
            .samples
            .iter()
            .filter(|s| s.inputs.iter().any(|iid| iid == id))
            .map(|s| s.name.clone())
            .collect();
        if !refs.is_empty() {
            return Err(io::Error::other(format!(
                "input '{}' is referenced by sample(s): {}; unlink them first",
                id,
                refs.join(", ")
            )));
        }

        self.inputs.remove(idx);
        self.save()?;
        Ok(())
    }

    // ======================================================================
    // Sample registry (P2)
    // ======================================================================

    /// Create a new sample. Input ids must exist in `self.inputs`.
    pub fn create_sample(
        &mut self,
        name: String,
        group: Option<String>,
        condition: Option<String>,
        input_ids: Vec<String>,
    ) -> io::Result<SampleRecord> {
        for iid in &input_ids {
            if !self.inputs.iter().any(|r| r.id == *iid) {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("input '{}' not found", iid),
                ));
            }
        }
        let paired = input_ids.len() >= 2;
        let rec = SampleRecord {
            id: new_sample_id(),
            name,
            group,
            condition,
            inputs: input_ids,
            paired,
            notes: None,
        };
        self.samples.push(rec.clone());
        self.save()?;
        Ok(rec)
    }

    pub fn update_sample(&mut self, id: &str, patch: SamplePatch) -> io::Result<SampleRecord> {
        // Validate input refs in the patch (if any) before mutating.
        if let Some(ref inputs) = patch.inputs {
            for iid in inputs {
                if !self.inputs.iter().any(|r| r.id == *iid) {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("input '{}' not found", iid),
                    ));
                }
            }
        }
        let rec = self
            .samples
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("sample '{}' not found", id),
                )
            })?;
        if let Some(name) = patch.name {
            rec.name = name;
        }
        if let Some(group) = patch.group {
            rec.group = if group.is_empty() { None } else { Some(group) };
        }
        if let Some(cond) = patch.condition {
            rec.condition = if cond.is_empty() { None } else { Some(cond) };
        }
        if let Some(inputs) = patch.inputs {
            rec.paired = inputs.len() >= 2;
            rec.inputs = inputs;
        }
        if let Some(notes) = patch.notes {
            rec.notes = if notes.is_empty() { None } else { Some(notes) };
        }
        let out = rec.clone();
        self.save()?;
        Ok(out)
    }

    pub fn delete_sample(&mut self, id: &str) -> io::Result<()> {
        let idx = self
            .samples
            .iter()
            .position(|s| s.id == id)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("sample '{}' not found", id),
                )
            })?;
        self.samples.remove(idx);
        self.save()?;
        Ok(())
    }

    /// Build samples by auto-pairing `_R1` / `_R2` among the project's
    /// already-registered FASTQ inputs. Skips inputs that are already part
    /// of a sample. Returns the newly created samples.
    pub fn auto_pair_samples(&mut self) -> io::Result<Vec<SampleRecord>> {
        use crate::sample::pair_fastq_names;
        let already_linked: std::collections::HashSet<String> = self
            .samples
            .iter()
            .flat_map(|s| s.inputs.iter().cloned())
            .collect();

        // Map of file_name -> input_id
        let candidates: Vec<(String, String)> = self
            .inputs
            .iter()
            .filter(|r| r.kind == InputKind::Fastq && !r.missing)
            .filter(|r| !already_linked.contains(&r.id))
            .map(|r| {
                (
                    r.path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string(),
                    r.id.clone(),
                )
            })
            .collect();

        let names: Vec<String> = candidates.iter().map(|(n, _)| n.clone()).collect();
        let groups = pair_fastq_names(&names);

        let mut created = Vec::new();
        for (stem, members) in groups {
            let mut input_ids: Vec<String> = Vec::new();
            for name in &members {
                if let Some((_, id)) = candidates.iter().find(|(n, _)| n == name) {
                    input_ids.push(id.clone());
                }
            }
            if input_ids.is_empty() {
                continue;
            }
            let rec = SampleRecord {
                id: new_sample_id(),
                name: stem,
                group: None,
                condition: None,
                paired: input_ids.len() >= 2,
                inputs: input_ids,
                notes: None,
            };
            self.samples.push(rec.clone());
            created.push(rec);
        }
        if !created.is_empty() {
            self.save()?;
        }
        Ok(created)
    }

    /// Import samples from a TSV/CSV sample sheet. Expected columns
    /// (case-insensitive, in any order):
    ///   - `sample_id` (required) — logical sample name
    ///   - `r1`        (required) — path to the R1 fastq (also auto-registered as an Input)
    ///   - `r2`        (optional) — path to the R2 fastq; presence → paired_end
    ///   - `group`, `condition`, `notes` — optional metadata
    ///
    /// Returns the created SampleRecords plus per-row error messages.
    /// Already-registered paths are reused (no duplicates); unknown paths
    /// are auto-registered as Inputs with Fastq kind.
    pub fn import_samples_from_tsv(
        &mut self,
        path: &Path,
    ) -> io::Result<(Vec<SampleRecord>, Vec<String>)> {
        let data = fs::read_to_string(path)?;
        let mut lines = data.lines().filter(|l| !l.trim().is_empty());
        let header_line = lines
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "empty sample sheet"))?;

        let sep = if header_line.contains('\t') {
            '\t'
        } else {
            ','
        };
        let headers: Vec<String> = header_line
            .split(sep)
            .map(|s| s.trim().to_ascii_lowercase())
            .collect();
        let find = |name: &str| headers.iter().position(|h| h == name);
        let id_col = find("sample_id")
            .or_else(|| find("sample"))
            .or_else(|| find("name"))
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "missing sample_id column")
            })?;
        let r1_col = find("r1")
            .or_else(|| find("fastq_1"))
            .or_else(|| find("read1"))
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing r1 column"))?;
        let r2_col = find("r2")
            .or_else(|| find("fastq_2"))
            .or_else(|| find("read2"));
        let group_col = find("group");
        let cond_col = find("condition").or_else(|| find("treatment"));
        let notes_col = find("notes");

        let mut created = Vec::new();
        let mut errors = Vec::new();
        let proj_dir = self.root_dir.clone();

        for (idx, line) in lines.enumerate() {
            let row_no = idx + 2; // 1-indexed + header
            let cols: Vec<&str> = line.split(sep).map(|s| s.trim()).collect();
            let get = |i: usize| cols.get(i).map(|s| s.to_string()).unwrap_or_default();

            let sample_id = get(id_col);
            if sample_id.is_empty() {
                errors.push(format!("row {}: empty sample_id", row_no));
                continue;
            }
            let r1_raw = get(r1_col);
            if r1_raw.is_empty() {
                errors.push(format!("row {}: empty r1", row_no));
                continue;
            }
            let r2_raw = r2_col.map(get).filter(|s| !s.is_empty());

            let mut input_ids = Vec::new();
            let mut any_registered = false;
            for raw in [Some(r1_raw), r2_raw].into_iter().flatten() {
                let p = resolve_maybe_relative(&proj_dir, Path::new(&raw));
                match self.register_input_inner(&p) {
                    Ok((rec, was_new)) => {
                        if was_new {
                            any_registered = true;
                        }
                        input_ids.push(rec.id);
                    }
                    Err(e) => {
                        errors.push(format!("row {}: cannot register '{}': {}", row_no, raw, e));
                    }
                }
            }
            if input_ids.is_empty() {
                continue;
            }
            if any_registered {
                // register_input_inner doesn't persist on its own; save after each row with new inputs
                // (cheap; typical sheets are small). Skip in a tight future optimization.
                let _ = self.save();
            }

            let group = group_col.map(get).filter(|s| !s.is_empty());
            let condition = cond_col.map(get).filter(|s| !s.is_empty());
            let notes = notes_col.map(get).filter(|s| !s.is_empty());

            match self.create_sample(sample_id.clone(), group, condition, input_ids) {
                Ok(mut rec) => {
                    rec.notes = notes;
                    // Push the notes update.
                    if let Some(last) = self.samples.iter_mut().find(|s| s.id == rec.id) {
                        last.notes = rec.notes.clone();
                    }
                    let _ = self.save();
                    created.push(rec);
                }
                Err(e) => {
                    errors.push(format!("row {} '{}': {}", row_no, sample_id, e));
                }
            }
        }
        Ok((created, errors))
    }

    // ======================================================================
    // Asset registry (P3)
    // ======================================================================

    /// Register an `AssetRecord` produced by `run_id`. Paths declared as
    /// `relative_path` resolve against the run's output directory.
    /// Missing output files are silently skipped (the run may have produced
    /// a subset of what it declared, and we don't want to fail the whole
    /// run just because one optional artifact didn't materialize).
    pub fn register_declared_assets(
        &mut self,
        run_id: &str,
        declared: &[DeclaredAsset],
    ) -> io::Result<Vec<String>> {
        let run_dir = match self.run_dir(run_id) {
            Some(d) => d,
            None => return Ok(Vec::new()),
        };
        let mut new_ids = Vec::new();
        for d in declared {
            let abs_path = run_dir.join(&d.relative_path);
            if !abs_path.exists() {
                continue;
            }
            let size_bytes = if abs_path.is_dir() {
                dir_size_bytes(&abs_path).unwrap_or(0)
            } else {
                fs::metadata(&abs_path).map(|m| m.len()).unwrap_or(0)
            };
            let rec = AssetRecord {
                id: new_asset_id(),
                kind: d.kind.clone(),
                path: abs_path,
                size_bytes,
                produced_by_run_id: run_id.to_string(),
                display_name: d.display_name.clone(),
                schema: d.schema.clone(),
                created_at: Utc::now(),
            };
            new_ids.push(rec.id.clone());
            self.assets.push(rec);
        }
        // Also record them on the RunRecord for fast lineage lookup.
        if let Some(run) = self.runs.iter_mut().find(|r| r.id == run_id) {
            run.assets_produced.extend(new_ids.clone());
        }
        if !new_ids.is_empty() {
            self.save()?;
        }
        Ok(new_ids)
    }

    pub fn delete_asset(&mut self, id: &str) -> io::Result<()> {
        let idx = self.assets.iter().position(|a| a.id == id).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, format!("asset '{}' not found", id))
        })?;
        // Unlink from runs' produced / used lists.
        for r in self.runs.iter_mut() {
            r.assets_produced.retain(|x| x != id);
            r.assets_used.retain(|x| x != id);
        }
        self.assets.remove(idx);
        self.save()?;
        Ok(())
    }

    pub fn asset_by_kind(&self, kind: &AssetKind) -> Vec<&AssetRecord> {
        self.assets.iter().filter(|a| a.kind == *kind).collect()
    }

    /// Re-stat every registered input, updating `size_bytes` + flipping the
    /// `missing` flag as files come and go. Returns a summary report.
    pub fn scan_inputs(&mut self) -> io::Result<InputScanReport> {
        let mut refreshed = 0u32;
        let mut now_missing = 0u32;
        let mut recovered = 0u32;

        for rec in self.inputs.iter_mut() {
            match fs::metadata(&rec.path) {
                Ok(meta) => {
                    if rec.missing {
                        recovered += 1;
                        rec.missing = false;
                    }
                    if rec.size_bytes != meta.len() {
                        rec.size_bytes = meta.len();
                    }
                    refreshed += 1;
                }
                Err(_) => {
                    if !rec.missing {
                        now_missing += 1;
                        rec.missing = true;
                    }
                }
            }
        }

        self.save()?;
        Ok(InputScanReport {
            refreshed,
            now_missing,
            recovered,
        })
    }
}

/// If `p` is absolute, return it unchanged. Otherwise join against `project_dir`
/// so relative sample-sheet paths (e.g. `input/sample_R1.fastq.gz`) resolve
/// to real files on disk.
fn resolve_maybe_relative(project_dir: &Path, p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        project_dir.join(p)
    }
}

fn dir_size_bytes(path: &Path) -> io::Result<u64> {
    if !path.exists() {
        return Ok(0);
    }
    let mut total: u64 = 0;
    let mut stack = vec![path.to_path_buf()];
    while let Some(p) = stack.pop() {
        let meta = match fs::symlink_metadata(&p) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            let entries = match fs::read_dir(&p) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                stack.push(entry.path());
            }
        } else {
            total = total.saturating_add(meta.len());
        }
    }
    Ok(total)
}

#[cfg(test)]
mod project_default_view_tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn legacy_project_json_without_default_view_loads_as_manual() {
        let tmp = tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("runs")).unwrap();
        let legacy = r#"{
            "name": "legacy",
            "created_at": "2026-01-01T00:00:00Z",
            "runs": []
        }"#;
        std::fs::write(tmp.path().join("project.json"), legacy).unwrap();
        let proj = Project::load(tmp.path()).unwrap();
        assert_eq!(proj.default_view.as_deref(), Some("manual"));
    }

    #[test]
    fn newly_created_project_persists_default_view() {
        let tmp = tempdir().unwrap();
        let mut p = Project::create("t", tmp.path()).unwrap();
        p.default_view = Some("ai".into());
        p.save().unwrap();
        let reloaded = Project::load(tmp.path()).unwrap();
        assert_eq!(reloaded.default_view.as_deref(), Some("ai"));
    }
}

#[cfg(test)]
mod delete_run_tests {
    use super::*;
    use tempfile::tempdir;

    fn make_run(proj: &mut Project, status: RunStatus) -> String {
        let rec = proj.create_run("qc", serde_json::json!({}));
        let id = rec.id.clone();
        let run = proj.runs.iter_mut().find(|r| r.id == id).unwrap();
        run.status = status;
        id
    }

    #[test]
    fn delete_run_removes_record_and_directory() {
        let tmp = tempdir().unwrap();
        let mut p = Project::create("t", tmp.path()).unwrap();
        let id = make_run(&mut p, RunStatus::Done);
        let run_dir = tmp.path().join("runs").join(&id);
        assert!(run_dir.is_dir(), "run dir should exist before delete");
        std::fs::write(run_dir.join("output.txt"), "hello").unwrap();

        p.delete_run(&id).unwrap();

        assert!(!run_dir.exists(), "run dir should be removed");
        assert!(p.runs.iter().all(|r| r.id != id), "record should be gone");
        let reloaded = Project::load(tmp.path()).unwrap();
        assert!(reloaded.runs.iter().all(|r| r.id != id));
    }

    #[test]
    fn delete_run_refuses_while_running() {
        let tmp = tempdir().unwrap();
        let mut p = Project::create("t", tmp.path()).unwrap();
        let id = make_run(&mut p, RunStatus::Running);
        let err = p.delete_run(&id).expect_err("must refuse Running run");
        assert!(err.to_string().contains("cancel it first"));
        assert!(p.runs.iter().any(|r| r.id == id));
    }

    #[test]
    fn delete_run_errors_on_unknown_id() {
        let tmp = tempdir().unwrap();
        let mut p = Project::create("t", tmp.path()).unwrap();
        let err = p.delete_run("does_not_exist").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn project_serializes_without_legacy_inputs_field() {
        // Old projects created before P1 have no `inputs` key in project.json;
        // they must load as an empty inputs[] via #[serde(default)].
        let tmp = tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("runs")).unwrap();
        let legacy = r#"{
            "name": "legacy",
            "created_at": "2026-01-01T00:00:00Z",
            "runs": []
        }"#;
        std::fs::write(tmp.path().join("project.json"), legacy).unwrap();
        let proj = Project::load(tmp.path()).unwrap();
        assert!(proj.inputs.is_empty());
    }

    #[test]
    fn run_dir_size_counts_bytes() {
        let tmp = tempdir().unwrap();
        let mut p = Project::create("t", tmp.path()).unwrap();
        let id = make_run(&mut p, RunStatus::Done);
        let run_dir = tmp.path().join("runs").join(&id);
        std::fs::write(run_dir.join("a.txt"), b"12345").unwrap();
        std::fs::create_dir_all(run_dir.join("sub")).unwrap();
        std::fs::write(run_dir.join("sub/b.txt"), b"678").unwrap();
        let total = p.run_dir_size(&id);
        // params.json adds bytes too — assert it's at least our written payload
        assert!(total >= 5 + 3, "size={} should cover both files", total);
    }
}

#[cfg(test)]
mod input_registry_tests {
    use super::*;
    use tempfile::tempdir;

    fn write_file(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn register_assigns_prefixed_id_and_records_size() {
        let tmp = tempdir().unwrap();
        let mut p = Project::create("t", tmp.path()).unwrap();
        let f = tmp.path().join("reads.fastq");
        write_file(&f, b"@ABC\nACGT\n+\nIIII\n");
        let rec = p.register_input(&f, None, None).unwrap();
        assert!(rec.id.starts_with("in_"));
        assert_eq!(rec.kind, InputKind::Fastq);
        assert_eq!(rec.size_bytes as usize, b"@ABC\nACGT\n+\nIIII\n".len());
        assert_eq!(rec.display_name, "reads.fastq");
        assert!(!rec.missing);
    }

    #[test]
    fn register_is_idempotent_on_duplicate_path() {
        let tmp = tempdir().unwrap();
        let mut p = Project::create("t", tmp.path()).unwrap();
        let f = tmp.path().join("reads.fastq");
        write_file(&f, b"ok");
        let r1 = p.register_input(&f, None, None).unwrap();
        let r2 = p.register_input(&f, None, None).unwrap();
        assert_eq!(r1.id, r2.id, "duplicate register must return same record");
        assert_eq!(p.inputs.len(), 1);
    }

    #[test]
    fn register_rejects_missing_path() {
        let tmp = tempdir().unwrap();
        let mut p = Project::create("t", tmp.path()).unwrap();
        let err = p
            .register_input(&tmp.path().join("does_not_exist.fastq"), None, None)
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert!(p.inputs.is_empty());
    }

    #[test]
    fn delete_removes_record_but_keeps_file_on_disk() {
        let tmp = tempdir().unwrap();
        let mut p = Project::create("t", tmp.path()).unwrap();
        let f = tmp.path().join("reads.fq.gz");
        write_file(&f, b"data");
        let rec = p.register_input(&f, None, None).unwrap();
        p.delete_input(&rec.id).unwrap();
        assert!(p.inputs.is_empty());
        assert!(f.exists(), "file on disk must survive registration removal");
    }

    #[test]
    fn update_applies_only_provided_patch_fields() {
        let tmp = tempdir().unwrap();
        let mut p = Project::create("t", tmp.path()).unwrap();
        let f = tmp.path().join("reads.fastq");
        write_file(&f, b"ok");
        let rec = p.register_input(&f, None, None).unwrap();
        let updated = p
            .update_input(
                &rec.id,
                InputPatch {
                    display_name: Some("sample_01_R1".into()),
                    kind: None,
                    notes: Some("from flow cell A".into()),
                },
            )
            .unwrap();
        assert_eq!(updated.display_name, "sample_01_R1");
        assert_eq!(updated.notes.as_deref(), Some("from flow cell A"));
        // kind was not patched, so it keeps the detected value
        assert_eq!(updated.kind, InputKind::Fastq);
    }

    #[test]
    fn scan_flips_missing_and_recovered() {
        let tmp = tempdir().unwrap();
        let mut p = Project::create("t", tmp.path()).unwrap();
        let f = tmp.path().join("reads.fastq");
        write_file(&f, b"v1");
        let rec = p.register_input(&f, None, None).unwrap();

        // remove the file — scan should mark missing
        std::fs::remove_file(&f).unwrap();
        let r1 = p.scan_inputs().unwrap();
        assert_eq!(r1.now_missing, 1);
        assert_eq!(r1.recovered, 0);
        assert!(p.inputs.iter().find(|r| r.id == rec.id).unwrap().missing);

        // restore the file — scan should flip missing back
        write_file(&f, b"v2 larger");
        let r2 = p.scan_inputs().unwrap();
        assert_eq!(r2.recovered, 1);
        assert_eq!(r2.now_missing, 0);
        let reloaded = p.inputs.iter().find(|r| r.id == rec.id).unwrap();
        assert!(!reloaded.missing);
        assert_eq!(reloaded.size_bytes as usize, b"v2 larger".len());
    }

    #[test]
    fn register_batch_reports_partial_errors() {
        let tmp = tempdir().unwrap();
        let mut p = Project::create("t", tmp.path()).unwrap();
        let ok1 = tmp.path().join("a.fastq");
        let ok2 = tmp.path().join("b.fa");
        let missing = tmp.path().join("does_not_exist.gff");
        write_file(&ok1, b"x");
        write_file(&ok2, b"x");

        let (registered, errors) =
            p.register_inputs_batch(&[ok1.clone(), missing.clone(), ok2.clone()]);
        assert_eq!(registered.len(), 2);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].0, missing);
        assert_eq!(p.inputs.len(), 2);
    }
}

#[cfg(test)]
mod sample_registry_tests {
    use super::*;
    use tempfile::tempdir;

    fn setup() -> (tempfile::TempDir, Project, String, String) {
        let tmp = tempdir().unwrap();
        let mut p = Project::create("t", tmp.path()).unwrap();
        let f1 = tmp.path().join("sampleA_R1.fastq.gz");
        let f2 = tmp.path().join("sampleA_R2.fastq.gz");
        std::fs::write(&f1, b"data").unwrap();
        std::fs::write(&f2, b"data").unwrap();
        let i1 = p.register_input(&f1, None, None).unwrap().id;
        let i2 = p.register_input(&f2, None, None).unwrap().id;
        (tmp, p, i1, i2)
    }

    #[test]
    fn create_sample_with_two_inputs_is_paired() {
        let (_tmp, mut p, i1, i2) = setup();
        let s = p
            .create_sample(
                "sampleA".into(),
                Some("treat".into()),
                Some("IFN".into()),
                vec![i1.clone(), i2.clone()],
            )
            .unwrap();
        assert!(s.id.starts_with("sam_"));
        assert!(s.paired);
        assert_eq!(s.inputs, vec![i1, i2]);
        assert_eq!(s.group.as_deref(), Some("treat"));
    }

    #[test]
    fn create_sample_rejects_unknown_input_id() {
        let (_tmp, mut p, _i1, _i2) = setup();
        let err = p
            .create_sample("x".into(), None, None, vec!["in_missing".into()])
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert!(p.samples.is_empty());
    }

    #[test]
    fn delete_input_refuses_when_referenced_by_sample() {
        let (_tmp, mut p, i1, i2) = setup();
        p.create_sample("sampleA".into(), None, None, vec![i1.clone(), i2])
            .unwrap();
        let err = p.delete_input(&i1).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("referenced by sample"), "got: {msg}");
    }

    #[test]
    fn auto_pair_builds_samples_from_r1_r2_convention() {
        let (_tmp, mut p, _i1, _i2) = setup();
        let created = p.auto_pair_samples().unwrap();
        assert_eq!(created.len(), 1, "expected one paired sample");
        assert_eq!(created[0].name, "sampleA");
        assert!(created[0].paired);
    }

    #[test]
    fn auto_pair_skips_already_linked_inputs() {
        let (_tmp, mut p, i1, i2) = setup();
        p.create_sample("manual".into(), None, None, vec![i1, i2])
            .unwrap();
        let created = p.auto_pair_samples().unwrap();
        assert!(created.is_empty(), "already-linked inputs must be skipped");
    }

    #[test]
    fn update_sample_applies_patch_and_recomputes_paired() {
        let (_tmp, mut p, i1, i2) = setup();
        let s = p
            .create_sample("sampleA".into(), None, None, vec![i1.clone(), i2])
            .unwrap();
        let updated = p
            .update_sample(
                &s.id,
                SamplePatch {
                    group: Some("ctrl".into()),
                    inputs: Some(vec![i1]),
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(!updated.paired);
        assert_eq!(updated.group.as_deref(), Some("ctrl"));
    }

    #[test]
    fn asset_registration_and_orphan_detection_work_end_to_end() {
        let tmp = tempdir().unwrap();
        let mut p = Project::create("t", tmp.path()).unwrap();
        // Create a run + its run_dir with a dummy produced file
        let run = p.create_run("star_index", serde_json::json!({}));
        let run_dir = p.run_dir(&run.id).unwrap();
        std::fs::create_dir_all(run_dir.join("index")).unwrap();
        std::fs::write(run_dir.join("index/SA"), b"bytes").unwrap();

        let declared = vec![crate::asset::DeclaredAsset {
            kind: crate::asset::AssetKind::StarIndex,
            relative_path: std::path::PathBuf::from("index"),
            display_name: "GRCh38 STAR index".into(),
            schema: Some("STAR 2.7".into()),
        }];
        let ids = p.register_declared_assets(&run.id, &declared).unwrap();
        assert_eq!(ids.len(), 1);
        assert_eq!(p.assets.len(), 1);
        assert_eq!(p.assets[0].produced_by_run_id, run.id);

        // Should show up as an orphan of this run (nobody else uses it yet).
        let orphans = p.orphan_assets_if_run_deleted(&run.id);
        assert_eq!(orphans, ids);

        // Another run consumes it → no longer orphan.
        let consumer = p.create_run("star_align", serde_json::json!({}));
        if let Some(r) = p.runs.iter_mut().find(|r| r.id == consumer.id) {
            r.assets_used.push(ids[0].clone());
        }
        let orphans2 = p.orphan_assets_if_run_deleted(&run.id);
        assert!(orphans2.is_empty());

        // Deleting the asset unlinks it from the consumer run.
        p.delete_asset(&ids[0]).unwrap();
        assert!(p.assets.is_empty());
        let consumer_after = p.runs.iter().find(|r| r.id == consumer.id).unwrap();
        assert!(consumer_after.assets_used.is_empty());
    }

    #[test]
    fn import_samples_from_tsv_creates_records_and_registers_inputs() {
        let tmp = tempdir().unwrap();
        let mut p = Project::create("t", tmp.path()).unwrap();
        let input_dir = tmp.path().join("input");
        let r1a = input_dir.join("sampleA_R1.fastq.gz");
        let r2a = input_dir.join("sampleA_R2.fastq.gz");
        let r1b = input_dir.join("sampleB_R1.fastq.gz");
        std::fs::write(&r1a, b"x").unwrap();
        std::fs::write(&r2a, b"x").unwrap();
        std::fs::write(&r1b, b"x").unwrap();

        let sheet = tmp.path().join("samples.tsv");
        // Relative paths (resolved against project_dir) + mix absolute for robustness.
        std::fs::write(
            &sheet,
            format!(
                "sample_id\tgroup\tcondition\tr1\tr2\n\
                 sampleA\ttreat\tIFN\tinput/sampleA_R1.fastq.gz\tinput/sampleA_R2.fastq.gz\n\
                 sampleB\tctrl\t\t{}\t\n",
                r1b.display()
            ),
        )
        .unwrap();

        let (created, errors) = p.import_samples_from_tsv(&sheet).unwrap();
        assert_eq!(created.len(), 2, "errors: {:?}", errors);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        // Both samples registered, total 3 inputs (2 for A, 1 for B)
        assert_eq!(p.inputs.len(), 3);
        let a = created.iter().find(|s| s.name == "sampleA").unwrap();
        assert!(a.paired);
        assert_eq!(a.group.as_deref(), Some("treat"));
        assert_eq!(a.condition.as_deref(), Some("IFN"));
        let b = created.iter().find(|s| s.name == "sampleB").unwrap();
        assert!(!b.paired);
        assert_eq!(b.group.as_deref(), Some("ctrl"));
    }

    #[test]
    fn import_samples_from_tsv_reports_missing_columns() {
        let tmp = tempdir().unwrap();
        let mut p = Project::create("t", tmp.path()).unwrap();
        let sheet = tmp.path().join("bad.tsv");
        std::fs::write(&sheet, "foo\tbar\n1\t2\n").unwrap();
        let err = p.import_samples_from_tsv(&sheet).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn import_samples_tsv_handles_comma_separator() {
        let tmp = tempdir().unwrap();
        let mut p = Project::create("t", tmp.path()).unwrap();
        let f = tmp.path().join("s1_R1.fq");
        std::fs::write(&f, b"x").unwrap();
        let sheet = tmp.path().join("s.csv");
        std::fs::write(&sheet, format!("sample_id,r1\ns1,{}\n", f.display())).unwrap();
        let (created, _errs) = p.import_samples_from_tsv(&sheet).unwrap();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].name, "s1");
    }

    #[test]
    fn legacy_project_without_samples_field_loads_as_empty() {
        let tmp = tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("runs")).unwrap();
        let legacy = r#"{
            "name": "legacy",
            "created_at": "2026-01-01T00:00:00Z",
            "runs": []
        }"#;
        std::fs::write(tmp.path().join("project.json"), legacy).unwrap();
        let p = Project::load(tmp.path()).unwrap();
        assert!(p.samples.is_empty());
    }
}
