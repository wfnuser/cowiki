//! Agent skills — builtin SKILL.md constants, lookup, installation.

use std::path::Path;

use crate::types::error::AgentError;

// ── Builtin skill constants ───────────────────────────────────

/// Shared cowiki CLI command reference.
pub const COWIKI_CLI_SKILL: &str = include_str!("../../../../cli/skills/cowiki-cli/commands.md");

pub const COMPILER_SKILL: &str = include_str!("../../skills/compiler/SKILL.md");
pub const DEEP_COMPILE_SKILL: &str = include_str!("../../skills/deep-compile/SKILL.md");
pub const REVIEW_SKILL: &str = include_str!("../../skills/review/SKILL.md");

pub fn lookup_skill(name: &str) -> Option<&'static str> {
    match name {
        "cowiki-cli" => Some(COWIKI_CLI_SKILL),
        "cowiki-compiler" => Some(COMPILER_SKILL),
        "cowiki-deep-compile" => Some(DEEP_COMPILE_SKILL),
        "cowiki-review" => Some(REVIEW_SKILL),
        _ => None,
    }
}

/// Install skills to `{agent_home}/skills/{name}/SKILL.md`.
/// `cowiki-cli` is always installed first as the base tool reference.
pub async fn install_skills(agent_home: &Path, skills: &[&str]) -> Result<(), AgentError> {
    let skills_dir = agent_home.join("skills");
    tokio::fs::create_dir_all(&skills_dir).await.map_err(|e| {
        AgentError::InvalidConfig(format!("skills dir {}: {e}", skills_dir.display()))
    })?;

    install_one(&skills_dir, "cowiki-cli", COWIKI_CLI_SKILL).await?;

    for name in skills {
        let content = match lookup_skill(name) {
            Some(c) => c,
            None => {
                tracing::warn!(%name, "unknown skill");
                continue;
            }
        };
        install_one(&skills_dir, name, content).await?;
    }
    Ok(())
}

async fn install_one(skills_dir: &Path, name: &str, content: &str) -> Result<(), AgentError> {
    let dir = skills_dir.join(name);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| AgentError::InvalidConfig(format!("skill dir {}: {e}", dir.display())))?;
    tokio::fs::write(dir.join("SKILL.md"), content)
        .await
        .map_err(|e| AgentError::InvalidConfig(format!("write {name}/SKILL.md: {e}")))?;
    tracing::info!(%name, "skill installed");
    Ok(())
}
