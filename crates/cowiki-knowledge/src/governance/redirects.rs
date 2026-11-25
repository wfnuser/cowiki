use super::{digest, eligible_concept, Citation, FilePatch, RepairProposal, TextEdit};
use crate::{
    knowledge_index::{self as index, FileIndex, LinkKind},
    link_syntax::*,
    okf,
};
use std::collections::{BTreeMap, BTreeSet};

/// Redirect field verbatim in the immutable source snapshot.
pub type RedirectFields = BTreeMap<String, (String, Citation)>;

fn canonical(
    first: &str,
    files: &FileIndex,
    fields: &RedirectFields,
) -> Option<(String, Vec<Citation>)> {
    let mut target = first.to_owned();
    let mut seen = BTreeSet::new();
    let mut evidence = Vec::new();
    while let Some((raw, location)) = fields.get(&target) {
        if seen.len() >= 16 || !seen.insert(target.clone()) || raw.contains(['?', '#']) {
            return None;
        }
        target = index::resolve_markdown_target(&target, raw)?;
        evidence.push(location.clone());
    }
    (eligible_concept(&target) && files.contains(&target) && !evidence.is_empty())
        .then_some((target, evidence))
}

pub fn propose_redirects(
    path: &str,
    text: &str,
    files: &FileIndex,
    fields: &RedirectFields,
) -> Option<RepairProposal> {
    redirect_patch(path, text, files, fields, None)
}

/// Restrict automatic repair to the exact candidate lines and stub sources
/// approved by the selected detector(s).
pub fn redirect_patch(
    path: &str,
    text: &str,
    files: &FileIndex,
    fields: &RedirectFields,
    approved: Option<&BTreeSet<(usize, String)>>,
) -> Option<RepairProposal> {
    if !eligible_concept(path) {
        return None;
    }
    let (_, body) = okf::split_frontmatter(text).ok()?;
    let prefix = text.len() - body.len();
    let mut edits = Vec::new();
    let mut sources = BTreeSet::new();
    for link in index::link_occurrences(body, path) {
        let (mut span, stub) = match link.kind {
            LinkKind::Markdown => {
                if body[link.range.clone()].matches("](").count() != 1 {
                    continue;
                }
                let Some(span) = inline_destination(body, &link.range) else {
                    continue;
                };
                // Reject reference-style links and Markdown escapes whose raw
                // destination does not round-trip to this parsed target.
                if index::resolve_markdown_target(path, &body[span.clone()]).as_deref()
                    != Some(&link.target)
                {
                    continue;
                }
                (span, link.target)
            }
            LinkKind::Wiki => {
                let Some(target) = files.resolve_wikilink(&link.target, path) else {
                    continue;
                };
                if !link.target.contains('/')
                    && files
                        .files()
                        .iter()
                        .filter(|f| {
                            f.rsplit('/')
                                .next()
                                .is_some_and(|name| name.eq_ignore_ascii_case(&link.target))
                        })
                        .count()
                        > 1
                {
                    continue;
                }
                let start = link.range.start + 2;
                let inner = &body[start..link.range.end - 2];
                let end = inner.find('|').unwrap_or(inner.len());
                (
                    start..start + inner[..end].trim_end_matches('\\').len(),
                    target,
                )
            }
        };
        let line = 1 + text[..prefix + span.start]
            .bytes()
            .filter(|c| *c == b'\n')
            .count();
        if approved.is_some_and(|set| !set.contains(&(line, stub.clone()))) {
            continue;
        }
        let Some((target, proof)) = canonical(&stub, files, fields) else {
            continue;
        };
        let raw = &body[span.clone()];
        span.start += raw.len() - raw.trim_start().len();
        let name = raw.trim_start().split(['#', '?']).next()?.trim_end();
        span.end = span.start + name.len();
        if span.is_empty() {
            continue;
        }
        let replacement = match link.kind {
            LinkKind::Wiki => {
                if target.contains(['|', '[', ']', '#', '?', '\n', '\r']) {
                    continue;
                }
                let replace = wikilink_text(name, &target, path, files);
                if index::resolve_wikilink_target(&replace)
                    .and_then(|s| files.resolve_wikilink(&s, path))
                    .as_deref()
                    != Some(&target)
                {
                    continue;
                }
                replace
            }
            LinkKind::Markdown => {
                let canonical = if name.starts_with('/') {
                    format!("/{target}")
                } else {
                    relative_markdown_path(path, &target)
                };
                let replace = encode_destination(&canonical, body[..span.start].ends_with('<'))
                    .replace('#', "%23")
                    .replace('?', "%3F");
                if index::resolve_markdown_target(path, &replace).as_deref() != Some(&target) {
                    continue;
                }
                replace
            }
        };
        sources.extend(proof);
        edits.push(TextEdit {
            start: prefix + span.start,
            end: prefix + span.end,
            before: name.into(),
            after: replacement,
        });
    }
    if edits.is_empty() {
        return None;
    }
    edits.sort_by_key(|e| e.start);
    let patch = FilePatch {
        path: path.into(),
        preimage: digest(text),
        edits,
    };
    patch.preview(path, text).ok()?;
    Some(RepairProposal {
        rationale: "Update links to their existing canonical destinations.".into(),
        evidence: sources.into_iter().collect(),
        patch,
    })
}
