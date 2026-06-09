use crate::models::DuplicateWarning;

/// Check for duplicate pages by comparing embeddings.
/// Returns (slug, similarity) pairs for pages above the threshold.
/// This is a pure function that takes pre-fetched similar pages.
pub fn find_duplicates(new_slug: &str, similar_pages: &[(String, f64)]) -> Vec<DuplicateWarning> {
    similar_pages
        .iter()
        .filter(|(slug, _)| slug != new_slug)
        .map(|(slug, score)| DuplicateWarning {
            new_slug: new_slug.to_string(),
            existing_slug: slug.clone(),
            similarity: *score,
        })
        .collect()
}
