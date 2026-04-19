const EN: &str = include_str!("prompts/system_en.md");
const ZH: &str = include_str!("prompts/system_zh.md");

pub fn base_prompt(lang: &str) -> &'static str {
    match lang {
        "zh" => ZH,
        _ => EN,
    }
}

/// Combine the language-specific base prompt with the project snapshot.
/// The snapshot section header is stable so system prompts compose
/// deterministically across turns.
pub fn compose(lang: &str, snapshot: &str) -> String {
    format!("{}\n\n## Project state\n\n{}", base_prompt(lang), snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_falls_back_to_english_for_unknown_lang() {
        assert_eq!(base_prompt("jp"), base_prompt("en"));
    }

    #[test]
    fn base_selects_zh_when_requested() {
        assert_ne!(base_prompt("zh"), base_prompt("en"));
        assert!(base_prompt("zh").contains("RustBrain"));
    }

    #[test]
    fn compose_contains_base_and_snapshot_sections() {
        let out = compose("en", "snap-x");
        assert!(out.contains("RustBrain"));
        assert!(out.contains("## Project state"));
        assert!(out.contains("snap-x"));
    }
}
