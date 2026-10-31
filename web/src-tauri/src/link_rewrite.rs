//! Keep links working when a page or folder is renamed or moved, as Obsidian
//! does with "Automatically update internal links".
//!
//! A link is rewritten only when it pointed at a moved file before the move
//! and no longer resolves to it afterwards. Wikilinks keep their style (a bare
//! name stays a bare name when that is still unambiguous), headings, block
//! references and aliases; Markdown links keep their fragment and encoding.

use crate::knowledge_index::{
    code_ranges, resolve_markdown_target, resolve_wikilink_target, FileIndex,
};
use pulldown_cmark::{Event, Options, Parser, Tag};
use std::ops::Range;
use std::path::Path;

/// Where a path ended up after moving `from` (a file or folder) to `to`.
fn moved(path: &str, from: &str, to: &str) -> String {
    if path == from {
        to.to_string()
    } else if let Some(rest) = path.strip_prefix(&format!("{from}/")) {
        format!("{to}/{rest}")
    } else {
        path.to_string()
    }
}

fn folder_of(path: &str) -> &str {
    path.rsplit_once('/')
        .map(|(folder, _)| folder)
        .unwrap_or("")
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn is_markdown(path: &str) -> bool {
    path.to_lowercase().ends_with(".md")
}

/// The link text for `target` as seen from `source`, in the original style.
fn wikilink_text(original: &str, target: &str, source: &str, files: &FileIndex) -> String {
    let keep_extension = !is_markdown(target) || original.to_lowercase().ends_with(".md");
    let shown = |path: &str| -> String {
        if keep_extension {
            path.to_string()
        } else {
            path[..path.len() - 3].to_string()
        }
    };
    let name = shown(file_name(target));
    if !original.contains('/') {
        let resolves = resolve_wikilink_target(&name)
            .and_then(|stored| files.resolve_wikilink(&stored, source))
            .is_some_and(|resolved| resolved == target);
        if resolves {
            return name;
        }
    }
    shown(target)
}

fn relative_markdown_path(source: &str, target: &str) -> String {
    let from: Vec<&str> = folder_of(source)
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let to: Vec<&str> = target.split('/').collect();
    let common = from
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count();
    let mut parts: Vec<String> = vec!["..".to_string(); from.len() - common];
    parts.extend(to[common..].iter().map(|part| part.to_string()));
    parts.join("/")
}

/// Raw destination of an inline Markdown link `[text](dest "title")` inside
/// `range`, as a byte range of `body`.
fn inline_destination(body: &str, range: &Range<usize>) -> Option<Range<usize>> {
    let text = &body[range.clone()];
    if !text.ends_with(')') {
        return None;
    }
    let open = text.rfind("](")? + 2;
    let inner = &text[open..text.len() - 1];
    let leading = inner.len() - inner.trim_start().len();
    let inner_trimmed = inner.trim_start();
    let (start, length) = if let Some(rest) = inner_trimmed.strip_prefix('<') {
        (leading + 1, rest.find('>')?)
    } else {
        (
            leading,
            inner_trimmed
                .find(char::is_whitespace)
                .unwrap_or(inner_trimmed.len()),
        )
    };
    let begin = range.start + open + start;
    Some(begin..begin + length)
}

/// Rewrite the links in one page. `old_source` is where the page was before
/// the move, `new_source` where it is now.
fn rewrite_page(
    body: &str,
    old_source: &str,
    new_source: &str,
    from: &str,
    to: &str,
    before: &FileIndex,
    after: &FileIndex,
) -> Option<String> {
    let excluded = code_ranges(body);
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();

    // Wikilinks and embeds: [[target#fragment|alias]]
    let mut cursor = 0;
    while let Some(found) = body[cursor..].find("[[") {
        let start = cursor + found + 2;
        let Some(length) = body[start..].find("]]") else {
            break;
        };
        let inner = start..start + length;
        cursor = inner.end + 2;
        let raw = &body[inner.clone()];
        if raw.contains('\n')
            || excluded
                .iter()
                .any(|code| code.start < inner.end && inner.start < code.end)
        {
            continue;
        }
        let (target_part, alias) = match raw.split_once('|') {
            Some((target, alias)) => (target, Some(alias)),
            None => (raw, None),
        };
        // Inside tables Obsidian escapes the alias pipe: [[Note\\|alias]].
        let (target_part, escape) = match target_part.strip_suffix('\\') {
            Some(target) if alias.is_some() => (target, "\\"),
            _ => (target_part, ""),
        };
        let (page, fragment) = match target_part.find('#') {
            Some(index) => (&target_part[..index], &target_part[index..]),
            None => (target_part, ""),
        };
        let Some(stored) = resolve_wikilink_target(page) else {
            continue;
        };
        let Some(old_target) = before.resolve_wikilink(&stored, old_source) else {
            continue;
        };
        let expected = moved(&old_target, from, to);
        if after.resolve_wikilink(&stored, new_source).as_deref() == Some(expected.as_str()) {
            continue;
        }
        let mut text = wikilink_text(page.trim(), &expected, new_source, after);
        text.push_str(fragment);
        text.push_str(escape);
        if let Some(alias) = alias {
            text.push('|');
            text.push_str(alias);
        }
        edits.push((inner, text));
    }

    // Inline Markdown links to local files.
    let mut code_block_depth = 0;
    for (event, range) in Parser::new_ext(body, Options::all()).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(_)) => code_block_depth += 1,
            Event::End(pulldown_cmark::TagEnd::CodeBlock) => code_block_depth -= 1,
            Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. })
                if code_block_depth == 0 =>
            {
                let Some(old_target) = resolve_markdown_target(old_source, &dest_url) else {
                    continue;
                };
                if !before.contains(&old_target) {
                    continue;
                }
                let expected = moved(&old_target, from, to);
                if resolve_markdown_target(new_source, &dest_url).as_deref()
                    == Some(expected.as_str())
                {
                    continue;
                }
                let Some(span) = inline_destination(body, &range) else {
                    continue;
                };
                if body[span.clone()] != *dest_url {
                    continue;
                }
                let fragment = dest_url
                    .find(['#', '?'])
                    .map(|index| &dest_url[index..])
                    .unwrap_or("");
                let mut path = if dest_url.starts_with('/') {
                    format!("/{expected}")
                } else {
                    relative_markdown_path(new_source, &expected)
                };
                if dest_url.contains('%')
                    || (dest_url.contains(' ') && !body[..span.start].ends_with('<'))
                {
                    path = path.replace(' ', "%20");
                }
                path.push_str(fragment);
                edits.push((span, path));
            }
            _ => {}
        }
    }

    if edits.is_empty() {
        return None;
    }
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut updated = body.to_string();
    for (range, text) in edits {
        updated.replace_range(range, &text);
    }
    (updated != body).then_some(updated)
}

/// After `from` was renamed to `to` (Space-relative, `/`-separated), update
/// links in every Markdown page. Returns the pages that changed.
pub fn update_links_after_move(
    root: &Path,
    from: &str,
    to: &str,
    before: &FileIndex,
    after: &FileIndex,
    pages: &[String],
) -> Result<Vec<String>, String> {
    let mut changed = Vec::new();
    for page in pages.iter().filter(|page| is_markdown(page)) {
        // `pages` lists the Space after the move; find where each was before.
        let old_source = if page == to {
            from.to_string()
        } else if let Some(rest) = page.strip_prefix(&format!("{to}/")) {
            format!("{from}/{rest}")
        } else {
            page.clone()
        };
        let path = root.join(page);
        let Ok(body) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Some(updated) = rewrite_page(&body, &old_source, page, from, to, before, after) {
            let temporary =
                path.with_extension(format!("md.cowiki-{}.tmp", uuid::Uuid::new_v4().simple()));
            std::fs::write(&temporary, updated).map_err(|error| error.to_string())?;
            std::fs::rename(&temporary, &path).map_err(|error| error.to_string())?;
            changed.push(page.clone());
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(paths: &[&str]) -> FileIndex {
        FileIndex::new(paths.iter().map(|path| path.to_string()).collect())
    }

    fn rewrite(
        body: &str,
        source: &str,
        from: &str,
        to: &str,
        before: &[&str],
        after: &[&str],
    ) -> String {
        let new_source = moved(source, from, to);
        rewrite_page(
            body,
            source,
            &new_source,
            from,
            to,
            &index(before),
            &index(after),
        )
        .unwrap_or_else(|| body.to_string())
    }

    #[test]
    fn renaming_a_note_updates_name_links_and_keeps_fragments_and_aliases() {
        let before = ["Projects/Old Name.md", "Daily/2026-10-31.md", "img/a.png"];
        let after = ["Projects/New Name.md", "Daily/2026-10-31.md", "img/a.png"];
        let body = "See [[Old Name]], [[old name#Plan|the plan]], ![[Old Name]], [[Projects/Old Name.md#^abc]] and [x](../Projects/Old%20Name.md#top).\n\n`[[Old Name]]`\n";
        assert_eq!(
            rewrite(body, "Daily/2026-10-31.md", "Projects/Old Name.md", "Projects/New Name.md", &before, &after),
            "See [[New Name]], [[New Name#Plan|the plan]], ![[New Name]], [[Projects/New Name.md#^abc]] and [x](../Projects/New%20Name.md#top).\n\n`[[Old Name]]`\n"
        );
    }

    #[test]
    fn moving_a_note_keeps_unique_name_links_and_updates_paths() {
        let before = ["Inbox/Idea.md", "Notes/Index.md"];
        let after = ["Archive/Idea.md", "Notes/Index.md"];
        let body = "[[Idea]] [[Inbox/Idea]] [md](../Inbox/Idea.md)\n";
        assert_eq!(
            rewrite(
                body,
                "Notes/Index.md",
                "Inbox/Idea.md",
                "Archive/Idea.md",
                &before,
                &after
            ),
            "[[Idea]] [[Archive/Idea]] [md](../Archive/Idea.md)\n"
        );
    }

    #[test]
    fn a_name_that_becomes_ambiguous_gets_a_full_path() {
        let before = ["A/Note.md", "B/Other.md", "C/x.md"];
        let after = ["A/Note.md", "B/Note.md", "C/x.md"];
        let body = "[[Other]]\n";
        assert_eq!(
            rewrite(body, "C/x.md", "B/Other.md", "B/Note.md", &before, &after),
            "[[B/Note]]\n"
        );
    }

    #[test]
    fn moving_a_folder_updates_relative_links_inside_and_outside_it() {
        let before = ["Proj/Plan.md", "Proj/assets/fig.png", "Home.md"];
        let after = [
            "Archive/Proj/Plan.md",
            "Archive/Proj/assets/fig.png",
            "Home.md",
        ];
        let inside = "![](assets/fig.png) [home](../Home.md) [[Home]]\n";
        assert_eq!(
            rewrite(
                inside,
                "Proj/Plan.md",
                "Proj",
                "Archive/Proj",
                &before,
                &after
            ),
            "![](assets/fig.png) [home](../../Home.md) [[Home]]\n"
        );
        let outside = "[plan](Proj/Plan.md) [[Proj/Plan]] [[Plan]]\n";
        assert_eq!(
            rewrite(outside, "Home.md", "Proj", "Archive/Proj", &before, &after),
            "[plan](Archive/Proj/Plan.md) [[Proj/Plan]] [[Plan]]\n"
        );
    }

    #[test]
    fn escaped_pipes_in_tables_are_kept() {
        let before = ["Old.md", "T.md"];
        let after = ["New.md", "T.md"];
        let body =
            "| Paper | Note |\n| --- | --- |\n| [[Old\\|the old one]] | [[Old#Intro\\|intro]] |\n";
        assert_eq!(
            rewrite(body, "T.md", "Old.md", "New.md", &before, &after),
            "| Paper | Note |\n| --- | --- |\n| [[New\\|the old one]] | [[New#Intro\\|intro]] |\n"
        );
    }

    #[test]
    fn unrelated_and_unresolved_links_are_left_alone() {
        let before = ["Old.md", "Keep.md"];
        let after = ["New.md", "Keep.md"];
        let body = "[[Keep]] [[Missing]] [web](https://example.com/Old.md)\n```\n[[Old]]\n```\n";
        assert_eq!(
            rewrite(body, "Keep.md", "Old.md", "New.md", &before, &after),
            body
        );
    }
}
