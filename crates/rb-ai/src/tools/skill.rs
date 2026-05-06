//! Load L3 skill markdown files (frontmatter + body) into ToolDefs.
//! Frontmatter is YAML; body is the SOP text (passed to the agent as a
//! sub-task prompt when the skill tool is invoked).

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::error::AiError;
use crate::memory::layers::SkillMeta;
use crate::tools::{
    schema::{RiskLevel, ToolDef, ToolError},
    ToolContext, ToolEntry, ToolExecutor, ToolOutput, ToolRegistry,
};

#[derive(Debug, Clone)]
pub struct LoadedSkill {
    pub slug: String,
    pub meta: SkillMeta,
    pub body: String,
    pub source_path: PathBuf,
}

/// Parse one `.md` file with optional `---\n…\n---` YAML frontmatter.
pub fn parse_skill_file(path: &Path) -> Result<LoadedSkill, AiError> {
    let raw = std::fs::read_to_string(path)?;
    let (front, body) = split_frontmatter(&raw);
    let meta: SkillMeta = serde_yaml::from_str(&front)
        .map_err(|e| AiError::Config(format!("skill frontmatter ({}): {e}", path.display())))?;
    let slug = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| meta.name.clone());
    Ok(LoadedSkill {
        slug,
        meta,
        body: body.trim().to_string(),
        source_path: path.to_path_buf(),
    })
}

fn split_frontmatter(raw: &str) -> (String, String) {
    let trimmed = raw.trim_start();
    if !trimmed.starts_with("---") {
        return (String::new(), raw.to_string());
    }
    let after = &trimmed[3..];
    if let Some(end) = after.find("\n---") {
        let front = after[..end].trim_start_matches('\n').to_string();
        let body = after[end + 4..].trim_start_matches('\n').to_string();
        (front, body)
    } else {
        (String::new(), raw.to_string())
    }
}

/// Register all skills found in `<skills_dir>/*.md` as `skill_<slug>` tools.
pub fn register_dir(reg: &mut ToolRegistry, dir: &Path) -> Result<usize, AiError> {
    let mut n = 0;
    if !dir.exists() {
        return Ok(0);
    }
    for ent in std::fs::read_dir(dir)?.flatten() {
        let path = ent.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        if path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.starts_with('_'))
            .unwrap_or(false)
        {
            continue;
        }
        let s = parse_skill_file(&path)?;
        let tool = SkillTool {
            slug: s.slug.clone(),
            body: s.body.clone(),
            triggers: s.meta.triggers.clone(),
        };
        reg.register(ToolEntry {
            def: ToolDef {
                name: format!("skill_{}", s.slug.replace('-', "_")),
                description: s.meta.description.clone(),
                risk: RiskLevel::RunMid,
                params: s.meta.inputs_schema.clone(),
            },
            executor: std::sync::Arc::new(tool),
        });
        n += 1;
    }
    Ok(n)
}

struct SkillTool {
    slug: String,
    body: String,
    triggers: Vec<String>,
}

#[async_trait]
impl ToolExecutor for SkillTool {
    async fn execute(&self, args: &Value, _ctx: ToolContext<'_>) -> Result<ToolOutput, ToolError> {
        // Skill tools surface the SOP body + binding args. agent_loop will
        // pick this up and inject it as a nested user prompt.
        Ok(ToolOutput::Value(json!({
            "skill": self.slug,
            "triggers": self.triggers,
            "args": args,
            "sop": self.body,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parse_extracts_frontmatter_and_body() {
        let tmp = tempdir().unwrap();
        let p = tmp.path().join("rna-seq.md");
        std::fs::write(
            &p,
            "---\nname: rna-seq\ndescription: do rna-seq\ntriggers: [rna-seq]\n---\n\n## SOP\n1. step\n",
        )
        .unwrap();
        let s = parse_skill_file(&p).unwrap();
        assert_eq!(s.slug, "rna-seq");
        assert_eq!(s.meta.name, "rna-seq");
        assert!(s.body.contains("SOP"));
    }

    #[test]
    fn register_dir_skips_underscore_files() {
        let tmp = tempdir().unwrap();
        std::fs::write(
            tmp.path().join("rna-seq.md"),
            "---\nname: rna-seq\ndescription: x\n---\nbody",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("_index.md"),
            "---\nname: x\ndescription: x\n---\n",
        )
        .unwrap();
        let mut reg = ToolRegistry::new();
        let n = register_dir(&mut reg, tmp.path()).unwrap();
        assert_eq!(n, 1);
        assert!(reg.get("skill_rna_seq").is_some());
    }
}
