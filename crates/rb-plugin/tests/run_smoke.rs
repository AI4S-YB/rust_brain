//! Smoke test for ExternalToolModule using /bin/echo on Unix.
//! Skipped on non-Unix where /bin/echo isn't guaranteed.

#![cfg(unix)]

use rb_core::cancel::CancellationToken;
use rb_core::module::Module;
use rb_core::run_event::RunEvent;
use rb_plugin::{ExternalToolModule, PluginManifest};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

#[tokio::test]
async fn echo_plugin_runs_to_completion() {
    let manifest: PluginManifest =
        toml::from_str(include_str!("data/echo_plugin.toml")).unwrap();
    let module = ExternalToolModule::new(Arc::new(manifest), PathBuf::from("/bin/echo"));

    let tmp = tempfile::tempdir().unwrap();
    let (tx, mut rx) = mpsc::channel::<RunEvent>(32);
    let cancel = CancellationToken::new();

    let result = module
        .run(&json!({"msg": "hello world"}), tmp.path(), tx, cancel)
        .await
        .expect("echo plugin should succeed");

    let mut saw = false;
    while let Ok(ev) = rx.try_recv() {
        if let RunEvent::Log { line, .. } = ev {
            if line.contains("hello world") {
                saw = true;
            }
        }
    }
    assert!(saw, "expected stdout log line containing the echoed message");
    assert_eq!(result.summary["exit_code"], 0);
}

#[tokio::test]
async fn missing_required_param_validates_out() {
    let manifest: PluginManifest =
        toml::from_str(include_str!("data/echo_plugin.toml")).unwrap();
    let module = ExternalToolModule::new(Arc::new(manifest), PathBuf::from("/bin/echo"));
    let errs = module.validate(&json!({}));
    assert_eq!(errs.len(), 1);
    assert_eq!(errs[0].field, "msg");
}
