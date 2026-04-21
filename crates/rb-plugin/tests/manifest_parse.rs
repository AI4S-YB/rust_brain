use rb_plugin::{CliRule, ParamType, PluginManifest};

#[test]
fn parses_rustqc_fixture() {
    let toml_str = include_str!("data/rustqc.toml");
    let m: PluginManifest = toml::from_str(toml_str).expect("parse rustqc manifest");

    assert_eq!(m.id, "rustqc");
    assert_eq!(m.name, "RustQC");
    assert_eq!(m.category.as_deref(), Some("qc"));
    assert_eq!(m.icon.as_deref(), Some("shield-check"));
    assert_eq!(m.version.as_deref(), Some("0.1.0"));
    assert_eq!(m.binary.id, "rustqc");
    assert_eq!(m.params.len(), 6);

    let input = &m.params[0];
    assert_eq!(input.name, "input_files");
    assert!(matches!(input.r#type, ParamType::FileList));
    assert!(input.required);
    match &input.cli {
        CliRule::Flag { flag, repeat_per_value, join_with } => {
            assert_eq!(flag, "-i");
            assert!(*repeat_per_value);
            assert!(join_with.is_none());
        }
        other => panic!("expected Flag rule, got {:?}", other),
    }

    let threads = &m.params[1];
    assert!(matches!(threads.r#type, ParamType::Integer));
    assert_eq!(threads.default, Some(serde_json::json!(4)));
    assert_eq!(threads.minimum, Some(1.0));

    let extra = m.params.iter().find(|p| p.name == "extra_args").unwrap();
    assert!(matches!(extra.cli, CliRule::Raw { .. }));

    assert_eq!(
        m.outputs.as_ref().unwrap().patterns,
        vec!["*.html", "*.json", "*.zip"]
    );

    let s = m.strings.as_ref().unwrap();
    assert_eq!(s.name_en.as_deref(), Some("RustQC"));
    assert_eq!(s.ai_hint_zh.as_deref(), Some("用 RustQC 做 FASTQ 质量评估。"));
}
