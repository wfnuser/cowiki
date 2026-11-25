//! Compose a single reviewable file transition. Only the caller that owns an
//! active AgentChange worktree can persist the returned bytes.
use super::{digest, FilePatch};
use crate::okf;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Component, Path},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReviewTransition {
    pub path: String,
    pub before: String,
    pub after: String,
}
impl ReviewTransition {
    pub fn preimage(&self) -> String {
        digest(&self.before)
    }
    pub fn postimage(&self) -> String {
        digest(&self.after)
    }
}
pub fn eligible_concept(path: &str) -> bool {
    path.ends_with(".md")
        && !path.contains(['\\', '\0', ':'])
        && path
            .split('/')
            .all(|p| !p.is_empty() && !p.starts_with('.'))
        && Path::new(path)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
        && okf::DocumentKind::from_path(Path::new(path)) == okf::DocumentKind::Concept
}

/// Only scalar top-level trust fields change. Reject complex representations
/// rather than reserializing somebody's unrelated metadata or comments.
fn demote(text: &str, as_of: &str) -> Result<String, String> {
    let (front, body) = okf::split_frontmatter(text)?;
    let yaml = front.ok_or("human-verified concept requires explicit frontmatter for staging")?;
    let fields = BTreeMap::from([
        ("verified", "false"),
        ("trust_tier", "agent_draft"),
        ("generated", "true"),
        ("updated_at", as_of),
    ]);
    let mut expected: serde_yaml::Mapping =
        serde_yaml::from_str(yaml).map_err(|e| e.to_string())?;
    let mut remaining = fields.clone();
    for (name, value) in &fields {
        let v = match *name {
            "verified" => serde_yaml::Value::Bool(false),
            "generated" => serde_yaml::Value::Bool(true),
            _ => serde_yaml::Value::String((*value).into()),
        };
        expected.insert((*name).into(), v);
    }
    let prefix = text.len() - body.len();
    let open = text.find('\n').ok_or("invalid YAML opening")? + 1;
    let newline = if text[..open].ends_with("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut rewritten = text[..open].to_string();
    for line in yaml.split_inclusive('\n') {
        let Some((key, suffix)) = line.split_once(':') else {
            rewritten.push_str(line);
            continue;
        };
        let Some(value) = remaining.remove(key.trim_end()) else {
            rewritten.push_str(line);
            continue;
        };
        let (raw, _) = suffix.split_once('#').unwrap_or((suffix, ""));
        let old = raw.trim();
        if old.starts_with(['|', '>', '&', '*', '[', '{', '!']) {
            return Err("complex trust field requires review".into());
        }
        let left = key.len() + 1 + raw.len() - raw.trim_start_matches([' ', '\t']).len();
        let right = left + old.len();
        rewritten.push_str(&line[..left]);
        if left == key.len() + 1 {
            rewritten.push(' ')
        }
        rewritten.push_str(value);
        if old.is_empty() && line[right..].starts_with('#') {
            rewritten.push(' ')
        }
        rewritten.push_str(&line[right..]);
    }
    for (key, value) in remaining {
        rewritten.push_str(&format!("{key}: {value}{newline}"));
    }
    rewritten.push_str(&text[open + yaml.len()..prefix]);
    rewritten.push_str(body);
    let (new, unchanged) = okf::split_frontmatter(&rewritten)?;
    let actual: serde_yaml::Mapping =
        serde_yaml::from_str(new.ok_or("missing rewritten YAML")?).map_err(|e| e.to_string())?;
    if actual != expected || unchanged != body {
        return Err("trust edits did not preserve the rest of the document".into());
    }
    Ok(rewritten)
}

pub fn prepare_review(
    patch: &FilePatch,
    base: &str,
    candidate: &str,
    as_of: &str,
) -> Result<ReviewTransition, String> {
    if !eligible_concept(&patch.path) || !okf::valid_iso_date(as_of) {
        return Err("review requires a visible concept path and ISO date".into());
    }
    let updated = patch.preview(&patch.path, candidate)?;
    if updated == candidate {
        return Err("proposal makes no change".into());
    }
    let reset = [base, candidate, &updated]
        .iter()
        .any(|doc| okf::parse_metadata(doc).is_human_verified());
    let after = if reset {
        demote(&updated, as_of)?
    } else {
        updated
    };
    Ok(ReviewTransition {
        path: patch.path.clone(),
        before: candidate.to_owned(),
        after,
    })
}

/// Selection from an immutable preview. The desktop independently reloads the
/// Git base and the active worktree before accepting it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewSelection {
    pub base_digest: String,
    pub candidate: String,
    pub proposal: super::RepairProposal,
}
