use sha2::{Digest, Sha256};

/// Build a URL-safe slug from a title. Falls back to a deterministic content-hash
/// slug (`page-<hash8>`) when the title has no slug-able characters (e.g. a
/// symbol/emoji-only title) — so such pages never collapse to an empty slug and
/// overwrite each other at `wiki/.md`. CJK/accented titles are kept (they are
/// Unicode-alphanumeric).
pub fn slug_for_title(title: &str, fallback_seed: &str) -> String {
    let slug = title
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != ' ', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        let hash = format!("{:x}", Sha256::digest(fallback_seed.as_bytes()));
        format!("page-{}", &hash[..8])
    } else {
        slug
    }
}

/// Canonical builder for a wiki page's markdown (frontmatter + body). Single source
/// of truth so every write path produces identical frontmatter (and never a
/// title-less page).
pub fn build_page_markdown(
    title: &str,
    summary: &str,
    kind: &str,
    sources: &[String],
    body: &str,
) -> String {
    let sources_yaml = sources
        .iter()
        .map(|s| format!("  - {s}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "---\ntitle: \"{title}\"\nsummary: \"{summary}\"\nkind: {kind}\nsources:\n{sources_yaml}\n---\n\n{body}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_normal() {
        assert_eq!(slug_for_title("Retry Patterns", "body"), "retry-patterns");
    }

    #[test]
    fn slug_keeps_cjk() {
        assert_eq!(slug_for_title("重试模式", "body"), "重试模式");
    }

    #[test]
    fn slug_falls_back_for_symbol_only_titles() {
        let a = slug_for_title("!!!", "content-a");
        let b = slug_for_title("***", "content-b");
        assert!(a.starts_with("page-"), "got {a}");
        assert!(b.starts_with("page-"), "got {b}");
        assert_ne!(a, b, "distinct content must yield distinct fallback slugs");
        // deterministic by seed (not by the symbolic title)
        assert_eq!(a, slug_for_title("@@@", "content-a"));
    }

    #[test]
    fn build_round_trips_fields() {
        let md = build_page_markdown("T", "S", "concept", &["src.md".into()], "Body.");
        assert!(md.contains("title: \"T\""));
        assert!(md.contains("summary: \"S\""));
        assert!(md.contains("kind: concept"));
        assert!(md.contains("  - src.md"));
        assert!(md.ends_with("Body."));
    }
}
