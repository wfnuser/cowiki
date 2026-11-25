//! Conservative comparison of two aligned prose sentences. Content anchors
//! locate a proposition. A separate, explicit change in a facet supplies the
//! contradiction; lexical similarity alone cannot supply one.
use super::Relation;
use std::collections::BTreeSet;

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .replace("can't", "cannot")
        .replace("doesn't", "does not")
        .split(|ch: char| !ch.is_alphanumeric() && !matches!(ch, '_' | '.' | '%' | '-'))
        .map(|s| s.trim_matches(['.', '-']))
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

fn stem(word: &str) -> String {
    if let Some(s) = word.strip_suffix("ies").filter(|s| s.len() > 2) {
        return format!("{s}y");
    }
    for tail in ["ing", "ed", "s"] {
        if let Some(s) = word
            .strip_suffix(tail)
            .filter(|s| s.len() > 3 && !s.ends_with('s'))
        {
            return s.trim_end_matches('e').to_owned();
        }
    }
    word.trim_end_matches('e').to_owned()
}

fn anchors(words: &[String]) -> BTreeSet<String> {
    words
        .iter()
        .map(String::as_str)
        .filter(|w| {
            w.len() > 2
                && !matches!(
                    *w,
                    "the"
                        | "and"
                        | "from"
                        | "that"
                        | "this"
                        | "with"
                        | "when"
                        | "for"
                        | "are"
                        | "was"
                        | "been"
                        | "which"
                        | "must"
                        | "cannot"
                        | "can"
                        | "may"
                        | "only"
                        | "any"
                        | "always"
                        | "never"
                        | "not"
                        | "should"
                        | "both"
                        | "than"
                        | "all"
                        | "into"
                        | "does"
                        | "under"
                        | "since"
                        | "after"
                        | "before"
                        | "will"
                        | "has"
                        | "have"
                        | "its"
                )
        })
        .map(stem)
        .collect()
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Force {
    Required,
    Permitted,
    Optional,
    Forbidden,
    Asserted,
}

fn force(tokens: &[String]) -> Force {
    let has = |term: &str| tokens.iter().any(|s| s == term);
    let pair = |a: &str, b: &str| tokens.windows(2).any(|w| w[0] == a && w[1] == b);
    if pair("not", "required") || pair("need", "not") || has("optional") {
        Force::Optional
    } else if pair("rules", "out")
        || pair("rule", "out")
        || has("cannot")
        || has("never")
        || has("forbidden")
        || has("unsupported")
        || has("disallows")
        || has("prohibited")
        || has("not")
    {
        Force::Forbidden
    } else if has("must") || has("required") || has("mandatory") {
        Force::Required
    } else if has("can") || has("may") || has("permitted") || has("allowed") {
        Force::Permitted
    } else {
        Force::Asserted
    }
}

fn opposed(a: Force, b: Force) -> bool {
    use Force::*;
    matches!(
        (a, b),
        (Forbidden, Required | Permitted | Asserted)
            | (Required | Permitted | Asserted, Forbidden)
            | (Required, Optional)
            | (Optional, Required)
    )
}

fn default_value(w: &[String]) -> Option<&str> {
    for (i, word) in w.iter().enumerate() {
        let tail = &w[i + 1..];
        let start = match word.as_str() {
            "defaults" if tail.first()?.as_str() == "to" => 1,
            "default" if tail.first()?.as_str() == "is" => 1,
            "default" if tail.starts_with(&["value".into(), "is".into()]) => 2,
            _ => continue,
        };
        let literal = tail.get(start)?.as_str();
        if !matches!(
            literal,
            "a" | "an" | "the" | "to" | "derived" | "determined" | "computed"
        ) {
            return Some(literal);
        }
    }
    None
}

pub(super) struct Sentence {
    words: Vec<String>,
    anchors: BTreeSet<String>,
}
impl Sentence {
    pub(super) fn new(text: &str) -> Self {
        let words = words(text);
        Self {
            anchors: anchors(&words),
            words,
        }
    }
    fn overlap(&self, other: &Self) -> (usize, usize) {
        (
            self.anchors.intersection(&other.anchors).count(),
            self.anchors.len().min(other.anchors.len()),
        )
    }
    fn closely_aligned(&self, other: &Self) -> bool {
        let (shared, short) = self.overlap(other);
        shared >= 2
            && shared * 2 >= short
            && shared * 4 >= self.anchors.union(&other.anchors).count()
    }
    fn guards(&self) -> BTreeSet<(&str, &str)> {
        self.words
            .windows(2)
            .filter(|w| matches!(w[0].as_str(), "if" | "unless" | "when" | "since" | "until"))
            .map(|w| (w[0].as_str(), w[1].as_str()))
            .collect()
    }
}

fn negated_predicate(words: &[String]) -> Option<String> {
    let i = words.iter().position(|s| {
        matches!(
            s.as_str(),
            "not"
                | "cannot"
                | "never"
                | "must"
                | "mandatory"
                | "required"
                | "optional"
                | "may"
                | "can"
        )
    })?;
    words[i + 1..]
        .iter()
        .find(|w| {
            !matches!(
                w.as_str(),
                "not" | "be" | "have" | "to" | "a" | "an" | "the"
            )
        })
        .map(|w| stem(w))
}

fn mentions_predicate(from: &Sentence, to: &Sentence) -> bool {
    negated_predicate(&from.words).is_some_and(|p| to.anchors.contains(&p))
}

fn has(words: &[String], list: &[&str]) -> bool {
    words.iter().any(|s| list.contains(&s.as_str()))
}

fn phrase(words: &[String], terms: &[&str]) -> bool {
    words
        .windows(terms.len())
        .any(|w| w.iter().zip(terms).all(|(a, b)| a == b))
}

fn release_guard(words: &[String]) -> Option<&str> {
    for (i, word) in words.iter().enumerate() {
        if matches!(
            word.as_str(),
            "since"
                | "from"
                | "until"
                | "before"
                | "after"
                | "version"
                | "introduced"
                | "deprecated"
                | "added"
        ) {
            let mut tail = &words[i + 1..];
            if tail.first().is_some_and(|w| w == "in") {
                tail = &tail[1..];
            }
            let named_release = tail
                .first()
                .is_some_and(|w| w == "release" || w == "version");
            if named_release {
                tail = &tail[1..];
            }
            if let Some(v) = tail.first().map(|v| v.trim_start_matches('v')) {
                let version_context = named_release
                    || matches!(
                        word.as_str(),
                        "version" | "introduced" | "deprecated" | "added"
                    )
                    || tail[0].starts_with('v')
                    || v.contains('.');
                if version_context
                    && !v.is_empty()
                    && v.split('.')
                        .all(|part| !part.is_empty() && part.bytes().all(|c| c.is_ascii_digit()))
                {
                    return Some(v);
                }
            }
        }
    }
    None
}

fn restriction(words: &[String]) -> Option<(bool, &[String])> {
    for (i, pair) in words.windows(2).enumerate() {
        if matches!(
            pair[0].as_str(),
            "only" | "restricted" | "limited" | "reserved" | "except"
        ) && matches!(pair[1].as_str(), "on" | "to" | "for" | "when" | "in")
        {
            return Some((pair[0] == "except", &words[i + 2..]));
        }
    }
    None
}

/// A subjectless follow-up ("the option", "it") can inherit its antecedent
/// from the preceding sentence in the same evidence claim.
pub(super) fn scope_relation(
    candidate: &str,
    source: &str,
    previous: Option<&str>,
) -> Option<Relation> {
    let a = Sentence::new(candidate);
    let b = Sentence::new(source);
    let (exception, terms) = restriction(&b.words)?;
    let refers = b.words.first().is_some_and(|w| w == "it" || w == "this")
        || b.words.starts_with(&["the".into(), "option".into()])
        || b.words.starts_with(&["the".into(), "feature".into()]);
    if !(a.closely_aligned(&b)
        || refers && previous.is_some_and(|prev| a.closely_aligned(&Sentence::new(prev))))
    {
        return None;
    }
    let domain = anchors(terms);
    let positive = matches!(
        force(&a.words),
        Force::Permitted | Force::Asserted | Force::Required
    );
    if exception {
        let excludes = domain.iter().any(|d| {
            a.words
                .iter()
                .any(|w| w.strip_prefix("non-").is_some_and(|x| stem(x) == *d))
        });
        if excludes && positive && force(&b.words) == Force::Forbidden {
            return Some(Relation::Contradiction);
        }
        return Some(Relation::Insufficient);
    }
    let universal = has(
        &a.words,
        &["arbitrary", "any", "every", "unconditionally", "regardless"],
    );
    let extends = phrase(&a.words, &["as", "on"])
        || phrase(&a.words, &["as", "well", "as"])
        || phrase(&a.words, &["also", "on"]);
    if positive && (universal || (extends && !a.anchors.is_disjoint(&domain))) {
        return Some(Relation::Contradiction);
    }
    None
}

fn value_after_number(w: &[String]) -> Vec<(&str, &str)> {
    w.windows(2)
        .filter(|p| p[0].parse::<u64>().is_ok() && p[1].chars().all(char::is_alphabetic))
        .map(|p| (p[1].as_str(), p[0].as_str()))
        .collect()
}

pub(super) fn facet_relation(candidate: &str, evidence: &str) -> Option<Relation> {
    let a = Sentence::new(candidate);
    let b = Sentence::new(evidence);
    let leading_scope = |w: &[String]| w.first().is_some_and(|x| x == "on" || x == "under");
    if leading_scope(&a.words) && leading_scope(&b.words) && a.words.get(1) != b.words.get(1) {
        return Some(Relation::ScopeDifference);
    }
    if !a.closely_aligned(&b) {
        return None;
    }
    if let (Some(left), Some(right)) = (release_guard(&a.words), release_guard(&b.words)) {
        if left != right {
            // A dated lifecycle event names one fact. A release applicability
            // clause instead limits where the rest of the assertion holds.
            let same_event = ["deprecated", "introduced", "added"]
                .iter()
                .any(|m| has(&a.words, &[m]) && has(&b.words, &[m]));
            return Some(if same_event {
                Relation::Contradiction
            } else {
                Relation::ScopeDifference
            });
        }
    }
    if restriction(&b.words).is_some() {
        if let Some(r) = scope_relation(candidate, evidence, None) {
            return Some(r);
        }
        return None;
    }
    if a.guards() != b.guards() {
        return Some(Relation::Insufficient);
    }
    if let (Some(left), Some(right)) = (default_value(&a.words), default_value(&b.words)) {
        if left != right {
            return Some(Relation::Contradiction);
        }
    }
    let left = value_after_number(&a.words);
    let right = value_after_number(&b.words);
    if let ([(unit, x)], [(other, y)]) = (left.as_slice(), right.as_slice()) {
        if unit == other && x != y && a.overlap(&b).0 >= 3 {
            return Some(Relation::Contradiction);
        }
    }
    if [a.words.as_slice(), b.words.as_slice()]
        .iter()
        .any(|w| phrase(w, &["not", "only"]))
    {
        return Some(Relation::Insufficient);
    }
    if opposed(force(&a.words), force(&b.words))
        && (mentions_predicate(&a, &b) || mentions_predicate(&b, &a))
    {
        return Some(Relation::Contradiction);
    }
    None
}

fn match_score(a: &Sentence, b: &Sentence) -> usize {
    let (shared, short) = a.overlap(b);
    if shared < 2 || shared * 2 < short {
        return 0;
    }
    200 * shared / (a.anchors.len() + b.anchors.len()).max(1)
}

fn equivalent_assertion(a: &Sentence, b: &Sentence) -> bool {
    a.closely_aligned(b)
        && force(&a.words) == force(&b.words)
        && a.guards() == b.guards()
        && !a.anchors.is_empty()
        && (a.anchors == b.anchors
            || (match_score(a, b) >= 65 && (mentions_predicate(a, b) || mentions_predicate(b, a))))
}

fn lost_qualifier(candidate: &str, prior: &str, new: &Sentence, old: &Sentence) -> bool {
    if let Some((false, _)) = restriction(&old.words) {
        if restriction(&new.words).is_none() {
            let before = prior.to_lowercase();
            if let Some((core, _)) = before.split_once(" only ") {
                // Removing just an exclusive scope from a corroborated base
                // statement generalizes its applicability.
                if words(candidate) == words(core) {
                    return true;
                }
            }
        }
    }
    if release_guard(&old.words).is_none() || release_guard(&new.words).is_some() {
        return false;
    }
    let core: BTreeSet<_> = old
        .anchors
        .iter()
        .filter(|s| {
            !s.chars().any(|c| c.is_ascii_digit())
                && !matches!(
                    s.as_str(),
                    "releas" | "version" | "introduc" | "add" | "deprecat"
                )
        })
        .collect();
    core.len() >= 2
        && core.iter().all(|s| new.anchors.contains(*s))
        && force(&new.words) == force(&old.words)
        && !has(&new.words, &["historically", "previously", "formerly"])
}

/// Align only with sentences removed from the same section of the base.
/// Tied antecedents are ambiguous. A base edit still needs corroboration from
/// the retrieved evidence before it can acquire a governance issue code.
pub(super) fn align_removed<'a>(
    candidate: &str,
    removed: impl Iterator<Item = &'a str>,
) -> Option<&'a str> {
    let query = Sentence::new(candidate);
    let mut ranks: Vec<_> = removed
        .map(|s| (match_score(&query, &Sentence::new(s)), s))
        .filter(|(n, _)| *n >= 42)
        .collect();
    ranks.sort_unstable_by(|a, b| b.cmp(a));
    ranks.dedup();
    let (best, s) = *ranks.first()?;
    if ranks.get(1).is_some_and(|(n, _)| best - n < 8) {
        return None;
    }
    Some(s)
}

pub(super) fn changed_facet(candidate: &str, prior: &str, source: &str) -> Option<Relation> {
    let new = Sentence::new(candidate);
    let old = Sentence::new(prior);
    let evidence = Sentence::new(source);
    // An intervening revision and its verbatim corroborating evidence are
    // sufficient even when a different clause accompanied the change.
    let supported = equivalent_assertion(&old, &evidence) || prior.eq_ignore_ascii_case(source);
    if supported
        && (lost_qualifier(candidate, prior, &new, &old)
            || facet_relation(candidate, prior) == Some(Relation::Contradiction))
    {
        return Some(Relation::Contradiction);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deleted_qualifier_needs_independent_support_and_unambiguous_alignment() {
        let candidate = "The exporter accepts packed frames.";
        let old = "The exporter accepts packed frames only on embedded hosts.";
        let other = "The importer accepts debug dumps.";
        assert_eq!(
            align_removed(candidate, [other, old].into_iter()),
            Some(old)
        );
        assert_eq!(
            changed_facet(candidate, old, old),
            Some(Relation::Contradiction)
        );
        assert_eq!(changed_facet(candidate, old, other), None);
        assert_eq!(
            align_removed(
                "The gateway accepts frames.",
                [
                    "The north gateway accepts frames.",
                    "The south gateway accepts frames."
                ]
                .into_iter()
            ),
            None
        );
    }
}
