use async_trait::async_trait;

use crate::{AuthStrategy, ExtractError, ExtractInput, ExtractMetadata, ExtractResult, SourceExtractor, SourceType};

pub struct GitHubExtractor {
    client: reqwest::Client,
}

impl GitHubExtractor {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    fn github_token(&self, input: &ExtractInput) -> Option<String> {
        input
            .config
            .get("GITHUB_TOKEN")
            .filter(|t| !t.is_empty())
            .cloned()
    }

    fn auth_header(&self, input: &ExtractInput) -> Option<String> {
        self.github_token(input)
            .map(|token| format!("Bearer {}", token))
    }

    async fn fetch_github_api(
        &self,
        url: &str,
        input: &ExtractInput,
    ) -> Result<serde_json::Value, ExtractError> {
        let mut req = self
            .client
            .get(url)
            .header("Accept", "application/vnd.github.v3+json")
            .header("User-Agent", "cowiki-extractor/0.1");

        if let Some(auth) = self.auth_header(input) {
            req = req.header("Authorization", &auth);
        }

        let resp = req.send().await.map_err(|e| {
            ExtractError::HttpError(format!("GitHub API request failed: {}", e))
        })?;

        let status = resp.status();
        let body: serde_json::Value = resp.json().await.unwrap_or_default();

        if !status.is_success() {
            if status.as_u16() == 401 || status.as_u16() == 403 {
                let msg = body["message"]
                    .as_str()
                    .unwrap_or("GitHub API returned 403 (rate limit likely exceeded)")
                    .to_string();
                return Err(ExtractError::AuthRequired(format!(
                    "GitHub API auth failed: {}. Set GITHUB_TOKEN in your settings or use a public repo.",
                    msg
                )));
            }
            if status.as_u16() == 404 {
                return Err(ExtractError::InvalidInput(
                    "GitHub resource not found (404). Check the URL.".into(),
                ));
            }
            return Err(ExtractError::HttpError(format!(
                "GitHub API HTTP {}: {}",
                status,
                body["message"].as_str().unwrap_or("unknown error")
            )));
        }

        Ok(body)
    }

    async fn extract_repo(&self, owner: &str, repo: &str, input: &ExtractInput) -> Result<ExtractResult, ExtractError> {
        // Fetch repo info
        let api_url = format!("https://api.github.com/repos/{}/{}", owner, repo);
        let repo_data = self.fetch_github_api(&api_url, input).await?;

        let description = repo_data["description"].as_str().unwrap_or("");
        let stars = repo_data["stargazers_count"].as_u64().unwrap_or(0);
        let language = repo_data["language"].as_str().unwrap_or("");

        // Fetch README
        let readme_url = format!("https://api.github.com/repos/{}/{}/readme", owner, repo);
        let mut readme_content = String::new();
        if let Ok(readme_data) = self.fetch_github_api(&readme_url, input).await {
            if let Some(content) = readme_data["content"].as_str() {
                if let Ok(decoded) = base64::Engine::decode(
                    &base64::engine::general_purpose::STANDARD,
                    content.replace('\n', ""),
                ) {
                    readme_content = String::from_utf8_lossy(&decoded).to_string();
                }
            }
        }

        // Fetch directory tree (top level)
        let tree_url = format!(
            "https://api.github.com/repos/{}/{}/git/trees/main?recursive=0",
            owner, repo
        );
        let mut tree_markdown = String::new();
        if let Ok(tree_data) = self.fetch_github_api(&tree_url, input).await {
            if let Some(tree) = tree_data["tree"].as_array() {
                tree_markdown.push_str("\n## Directory Structure\n\n");
                for entry in tree.iter().take(30) {
                    let path = entry["path"].as_str().unwrap_or("");
                    let entry_type = entry["type"].as_str().unwrap_or("");
                    let prefix = if entry_type == "tree" { "📁" } else { "📄" };
                    tree_markdown.push_str(&format!("- {} `{}`\n", prefix, path));
                }
            }
        }

        let mut markdown = String::new();
        markdown.push_str(&format!(
            "# {}/{}\n\n", owner, repo
        ));
        markdown.push_str(&format!("⭐ {} stars", stars));
        if !language.is_empty() {
            markdown.push_str(&format!(" · Language: {}", language));
        }
        markdown.push_str("\n\n");

        if !description.is_empty() {
            markdown.push_str(&format!("{}\n\n", description));
        }

        markdown.push_str("## README\n\n");
        markdown.push_str(&readme_content);
        markdown.push_str("\n\n");
        markdown.push_str(&tree_markdown);

        let mut metadata = ExtractMetadata::default();
        metadata.title = Some(format!("{}/{}", owner, repo));
        metadata.source_url = Some(api_url);

        Ok(ExtractResult {
            text: markdown,
            suggested_filename: format!("github-{}-{}.md", owner, repo),
            metadata,
            original_content: input.content.as_bytes().to_vec(),
        })
    }

    async fn extract_issue(&self, owner: &str, repo: &str, number: &str, input: &ExtractInput) -> Result<ExtractResult, ExtractError> {
        let api_url = format!(
            "https://api.github.com/repos/{}/{}/issues/{}",
            owner, repo, number
        );
        let issue_data = self.fetch_github_api(&api_url, input).await?;

        let title = issue_data["title"].as_str().unwrap_or("Untitled");
        let body = issue_data["body"].as_str().unwrap_or("");
        let state = issue_data["state"].as_str().unwrap_or("open");
        let author = issue_data["user"]["login"].as_str().unwrap_or("unknown");
        let created = issue_data["created_at"].as_str().unwrap_or("");
        let labels: Vec<&str> = issue_data["labels"]
            .as_array()
            .map(|a| a.iter().filter_map(|l| l["name"].as_str()).collect())
            .unwrap_or_default();

        // Fetch comments
        let comments_url = format!("{}/comments?per_page=30", api_url);
        let mut comments_md = String::new();
        if let Ok(comments) = self.fetch_github_api(&comments_url, input).await {
            if let Some(arr) = comments.as_array() {
                for comment in arr {
                    let comment_author = comment["user"]["login"].as_str().unwrap_or("unknown");
                    let comment_body = comment["body"].as_str().unwrap_or("");
                    let comment_date = comment["created_at"].as_str().unwrap_or("");
                    comments_md.push_str(&format!(
                        "\n### {} ({})\n\n{}\n",
                        comment_author, comment_date, comment_body
                    ));
                }
            }
        }

        let mut markdown = String::new();
        markdown.push_str(&format!("# {} (#{})\n\n", title, number));
        markdown.push_str(&format!(
            "**Author:** {} · **Status:** {} · **Created:** {}\n\n",
            author, state, created
        ));
        if !labels.is_empty() {
            markdown.push_str(&format!(
                "**Labels:** {}\n\n",
                labels.iter().map(|l| format!("`{}`", l)).collect::<Vec<_>>().join(" ")
            ));
        }
        markdown.push_str(&format!("## Description\n\n{}\n\n", body));
        if !comments_md.is_empty() {
            markdown.push_str(&format!("## Comments\n\n{}\n", comments_md));
        }

        let mut metadata = ExtractMetadata::default();
        metadata.title = Some(title.to_string());
        metadata.author = Some(author.to_string());
        metadata.source_url = Some(format!(
            "https://github.com/{}/{}/issues/{}",
            owner, repo, number
        ));

        Ok(ExtractResult {
            text: markdown,
            suggested_filename: format!("github-{}-{}-issue-{}.md", owner, repo, number),
            metadata,
            original_content: input.content.as_bytes().to_vec(),
        })
    }

    fn parse_github_url(url: &str) -> Option<(String, String, Option<String>)> {
        // Support formats:
        // - https://github.com/owner/repo
        // - https://github.com/owner/repo/issues/123
        // - owner/repo
        // - owner/repo#123
        let url = url.trim().trim_end_matches('/');

        // Strip protocol
        let path = url
            .strip_prefix("https://github.com/")
            .or_else(|| url.strip_prefix("http://github.com/"))
            .unwrap_or(url);

        let parts: Vec<&str> = path.split('/').collect();

        if parts.len() < 2 {
            return None;
        }

        let owner = parts[0].to_string();
        let repo = parts[1].to_string();
        let issue: Option<String> = if parts.len() >= 4 && parts[2] == "issues" {
            Some(parts[3].to_string())
        } else if let Some(hash_idx) = repo.find('#') {
            let issue_num = repo[hash_idx + 1..].to_string();
            Some(issue_num)
        } else {
            None
        };

        // Handle repo#issue format
        let (repo, issue) = if issue.is_some() {
            (repo, issue)
        } else if let Some(hash_idx) = repo.find('#') {
            let issue_num = Some(repo[hash_idx + 1..].to_string());
            (repo[..hash_idx].to_string(), issue_num)
        } else {
            (repo, None)
        };

        Some((owner, repo, issue))
    }
}

impl Default for GitHubExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SourceExtractor for GitHubExtractor {
    fn supported_types(&self) -> Vec<SourceType> {
        vec![SourceType::GitHubRepo, SourceType::GitHubIssue]
    }

    fn auth_strategy(&self) -> AuthStrategy {
        AuthStrategy::ApiKey
    }

    async fn extract(&self, input: ExtractInput) -> Result<ExtractResult, ExtractError> {
        let (owner, repo, issue) = Self::parse_github_url(&input.content)
            .ok_or_else(|| ExtractError::InvalidInput(
                "Could not parse GitHub URL. Expected format: owner/repo or owner/repo/issues/123".into(),
            ))?;

        match (&input.source_type, issue) {
            (SourceType::GitHubIssue, Some(num)) => {
                self.extract_issue(&owner, &repo, &num, &input).await
            }
            (_, _) => {
                self.extract_repo(&owner, &repo, &input).await
            }
        }
    }
}
