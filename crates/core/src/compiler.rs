use std::collections::HashMap;

use crate::ai::embedder::Embedder;
use crate::ai::llm::Llm;
use crate::ai::token_usage::TokenUsage;
use crate::ai::vlm::Vlm;
use crate::models::Page;

pub struct Compiler {
    llm: Box<dyn Llm>,
    vlm: Option<Box<dyn Vlm>>,
    embedder: Box<dyn Embedder>,
}

impl Compiler {
    pub fn new(llm: Box<dyn Llm>, vlm: Option<Box<dyn Vlm>>, embedder: Box<dyn Embedder>) -> Self {
        Self { llm, vlm, embedder }
    }

    pub async fn compile(&self, sources: &[(String, String)]) -> Result<Vec<Page>, String> {
        let combined = sources
            .iter()
            .map(|(name, content)| format!("## Source: {name}\n\n{content}"))
            .collect::<Vec<_>>()
            .join("\n\n---\n\n");

        let system = r#"You are a knowledge compiler. Given source documents, extract distinct concepts and produce wiki pages.

For each concept, output a markdown document with YAML frontmatter:

```
---
title: "Concept Title"
summary: "One-line summary"
sources:
  - source-filename.md
---

Content here with clear explanations.
```

Separate multiple pages with `===PAGE_BREAK===`.

Be concise. One concept per page. Use clear headings. Attribute claims to sources with `^[filename.md]`."#;

        let user = format!("Compile the following sources into wiki pages:\n\n{combined}");
        let result = self.llm.chat(system, &user).await?;

        let pages = result
            .content
            .split("===PAGE_BREAK===")
            .filter(|s: &&str| !s.trim().is_empty())
            .map(|raw: &str| parse_compiled_page(raw.trim()))
            .collect();

        Ok(pages)
    }

    pub async fn generate_summary(&self, content: &str) -> Result<String, String> {
        let resp = self
            .llm
            .chat(
                "Generate a one-line summary (max 100 chars) of this content. Return only the summary, nothing else.",
                content,
            )
            .await?;
        Ok(resp.content)
    }

    pub async fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        let result: crate::ai::embedder::EmbedResult = self.embedder.embed(text, false).await?;
        Ok(result.vector)
    }

    /// Get LLM token usage snapshot.
    pub fn llm_usage(&self) -> HashMap<String, TokenUsage> {
        self.llm.usage_snapshot()
    }

    /// Get VLM token usage snapshot (if configured).
    pub fn vlm_usage(&self) -> HashMap<String, TokenUsage> {
        self.vlm
            .as_ref()
            .map(|v| v.usage_snapshot())
            .unwrap_or_default()
    }

    /// Get embedder token usage snapshot.
    pub fn embedder_usage(&self) -> HashMap<String, TokenUsage> {
        self.embedder.usage_snapshot()
    }
}

fn parse_compiled_page(raw: &str) -> Page {
    let mut title = "Untitled".to_string();
    let mut summary = String::new();
    let mut sources = Vec::new();
    let mut body = raw.to_string();

    // Strip markdown code fences if present
    let raw = raw
        .trim_start_matches("```markdown")
        .trim_start_matches("```md")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    if raw.starts_with("---") {
        let parts: Vec<&str> = raw.splitn(3, "---").collect();
        if parts.len() >= 3 {
            let fm = parts[1];
            body = parts[2].trim().to_string();
            let mut in_sources = false;
            for line in fm.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("title:") {
                    title = trimmed
                        .trim_start_matches("title:")
                        .trim()
                        .trim_matches('"')
                        .to_string();
                    in_sources = false;
                } else if trimmed.starts_with("summary:") {
                    summary = trimmed
                        .trim_start_matches("summary:")
                        .trim()
                        .trim_matches('"')
                        .to_string();
                    in_sources = false;
                } else if trimmed.starts_with("sources:") {
                    in_sources = true;
                } else if in_sources && trimmed.starts_with("- ") {
                    sources.push(trimmed.trim_start_matches("- ").trim().to_string());
                } else {
                    in_sources = false;
                }
            }
        }
    }

    // Use the body as the fallback seed so a symbol-only title still gets a unique,
    // deterministic slug instead of collapsing to an empty slug (`wiki/.md`).
    let slug = crate::frontmatter::slug_for_title(&title, &body);

    Page {
        slug,
        title,
        summary,
        body,
        sources,
        created_at: chrono::Utc::now().to_rfc3339(),
    }
}
