use crate::models::DuplicateWarning;

/// Check for duplicate pages by comparing embeddings.
/// Returns (slug, similarity) pairs for pages above the threshold.
/// This is a pure function that takes pre-fetched similar pages.
pub fn find_duplicates(new_path: &str, similar_pages: &[(String, f64)]) -> Vec<DuplicateWarning> {
    similar_pages
        .iter()
        .filter(|(slug, _)| slug != new_path)
        .map(|(slug, score)| DuplicateWarning {
            new_path: new_path.to_string(),
            existing_path: slug.clone(),
            similarity: *score,
        })
        .collect()
}
