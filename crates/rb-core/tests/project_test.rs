use rb_core::project::{Project, RunStatus};
use tempfile::TempDir;

#[test]
fn create_and_load_project() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("test_project");

    // Create
    let mut project = Project::create("Test Experiment", &root).unwrap();
    assert_eq!(project.name, "Test Experiment");
    assert!(root.join("project.json").exists());
    assert!(root.join("input").is_dir());
    assert!(root.join("runs").is_dir());

    // Create a run
    let params = serde_json::json!({"threads": 4});
    let run = project.create_run("qc", params.clone());
    assert_eq!(run.module_id, "qc");
    assert_eq!(run.status, RunStatus::Pending);
    project.save().unwrap();

    // Verify run directory exists
    let run_dir = project.run_dir(&run.id).unwrap();
    assert!(run_dir.is_dir());
    assert!(run_dir.join("params.json").exists());

    // Load
    let loaded = Project::load(&root).unwrap();
    assert_eq!(loaded.name, "Test Experiment");
    assert_eq!(loaded.runs.len(), 1);
    assert_eq!(loaded.runs[0].module_id, "qc");
}
