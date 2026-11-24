//! Destination spelling shared by desktop moves and governance proposals.
use crate::knowledge_index::{resolve_wikilink_target, FileIndex};
use std::ops::Range;
fn folder_of(path: &str) -> &str {
    path.rsplit_once('/')
        .map(|(folder, _)| folder)
        .unwrap_or("")
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

pub fn is_markdown(path: &str) -> bool {
    path.to_lowercase().ends_with(".md")
}

/// The link text for `target` as seen from `source`, in the original style.
pub fn wikilink_text(original: &str, target: &str, source: &str, files: &FileIndex) -> String {
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

pub fn relative_markdown_path(source: &str, target: &str) -> String {
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
pub fn inline_destination(body: &str, range: &Range<usize>) -> Option<Range<usize>> {
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

/// Encode a new link destination so it still parses as one: the old link's
/// spelling says nothing about characters the new name brings in. Targets
/// are percent-decoded when resolved, so `%` itself is always encoded.
pub fn encode_destination(path: &str, angled: bool) -> String {
    let mut encoded = String::with_capacity(path.len());
    for character in path.chars() {
        match character {
            '%' => encoded.push_str("%25"),
            '<' => encoded.push_str("%3C"),
            '>' => encoded.push_str("%3E"),
            ' ' if !angled => encoded.push_str("%20"),
            '(' if !angled => encoded.push_str("%28"),
            ')' if !angled => encoded.push_str("%29"),
            character if character.is_control() => {
                let mut buffer = [0; 4];
                for byte in character.encode_utf8(&mut buffer).bytes() {
                    encoded.push_str(&format!("%{byte:02X}"));
                }
            }
            character => encoded.push(character),
        }
    }
    encoded
}
