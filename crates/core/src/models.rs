use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub body: String,
    pub sources: Vec<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateWarning {
    pub new_path: String,
    pub existing_path: String,
    pub similarity: f64,
}
