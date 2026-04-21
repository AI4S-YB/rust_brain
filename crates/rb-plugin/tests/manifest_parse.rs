use rb_plugin::{validate::validate_manifest, CliRule, ManifestIssueLevel, ParamType, PluginManifest};

fn parse(s: &str) -> rb_plugin::PluginManifest {
    toml::from_str(s).expect("parse")
}

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

#[test]
fn parses_positional_cli_rule() {
    let toml_str = r#"
id   = "tool"
name = "Tool"

[binary]
id = "tool"

[[params]]
name = "inputs"
type = "file_list"
cli  = { positional = true }
"#;
    let m: PluginManifest = toml::from_str(toml_str).expect("parse");
    let cli = &m.params[0].cli;
    assert!(cli.is_positional(), "expected Positional, got {:?}", cli);
    assert!(matches!(cli, CliRule::Positional { positional: true }));
}

#[test]
fn rustqc_fixture_is_valid() {
    let m = parse(include_str!("data/rustqc.toml"));
    let issues = validate_manifest(&m);
    let errors: Vec<_> = issues
        .iter()
        .filter(|i| i.level == ManifestIssueLevel::Error)
        .collect();
    assert!(errors.is_empty(), "fixture should validate, got {:?}", errors);
}

#[test]
fn rejects_duplicate_param_names() {
    let m = parse(
        r#"
        id = "x"
        name = "X"
        [binary]
        id = "x"
        [[params]]
        name = "a"
        type = "string"
        cli = { flag = "--a" }
        [[params]]
        name = "a"
        type = "integer"
        cli = { flag = "--a" }
        "#,
    );
    let issues = validate_manifest(&m);
    assert!(issues.iter().any(|i| i.field == "params[1].name" && i.level == ManifestIssueLevel::Error));
}

#[test]
fn rejects_unsupported_version() {
    let m = parse(
        r#"
        id = "x"
        name = "X"
        version = "9.9.9"
        [binary]
        id = "x"
        "#,
    );
    let issues = validate_manifest(&m);
    assert!(issues.iter().any(|i| i.field == "version" && i.level == ManifestIssueLevel::Error));
}

#[test]
fn rejects_enum_param_without_values() {
    let m = parse(
        r#"
        id = "x"
        name = "X"
        [binary]
        id = "x"
        [[params]]
        name = "fmt"
        type = "enum"
        cli = { flag = "--fmt" }
        "#,
    );
    let issues = validate_manifest(&m);
    assert!(issues.iter().any(|i| i.field.starts_with("params[0]") && i.level == ManifestIssueLevel::Error));
}

#[test]
fn rejects_required_with_default() {
    let m = parse(
        r#"
        id = "x"
        name = "X"
        [binary]
        id = "x"
        [[params]]
        name = "n"
        type = "integer"
        required = true
        default = 4
        cli = { flag = "--n" }
        "#,
    );
    let issues = validate_manifest(&m);
    assert!(issues.iter().any(|i| i.field.starts_with("params[0]") && i.level == ManifestIssueLevel::Error));
}
