# Cowiki-Extractor Proposal

## Overview

Extend cowiki's Ingest pipeline to support diverse source types beyond plain text and URLs. Each source type is handled by a dedicated **Extractor** that produces clean, structured Markdown text suitable for LLM wiki compilation.

**Status:** Proposed | **Issue:** [#31](https://github.com/wfnuser/cowiki/issues/31) | **Date:** 2026-06-06

---

## Architecture

### Crate Structure

```
crates/extractor/
├── Cargo.toml
└── src/
    ├── lib.rs          # Trait + enums + re-exports
    ├── error.rs        # ExtractError
    ├── types.rs        # ExtractInput, ExtractResult, ExtractMetadata
    ├── registry.rs     # ExtractorRegistry
    ├── text.rs         # TextExtractor (passthrough)
    ├── url.rs          # UrlExtractor (HTML → Markdown)
    ├── markdown.rs     # MarkdownExtractor (validate + normalize)
    ├── csv.rs          # CsvExtractor (CSV → Markdown table)
    ├── pdf.rs          # PdfExtractor (PDF → text)
    ├── docx.rs         # DocxExtractor (DOCX → Markdown)
    ├── pptx.rs         # PptxExtractor (PPTX → Markdown)
    ├── xlsx.rs         # XlsxExtractor (XLSX → Markdown table)
    ├── github.rs       # GitHubExtractor (API → Markdown)
    └── rss.rs          # RssExtractor (feed → Markdown)
```

### Core Trait

```rust
#[async_trait]
pub trait SourceExtractor: Send + Sync {
    fn supported_types(&self) -> Vec<SourceType>;
    fn auth_strategy(&self) -> AuthStrategy;
    async fn extract(&self, input: ExtractInput) -> Result<ExtractResult, ExtractError>;
}
```

### Key Types

**`SourceType`** — Enum of all supported source types:

```rust
pub enum SourceType {
    Auto,           // Auto-detect from filename/magic bytes
    Text,           // Plain text (passthrough)
    Url,            // Web page URL
    Pdf,
    Docx,
    Pptx,
    Xlsx,
    Csv,
    Markdown,
    GitHubRepo,     // GitHub repository
    GitHubIssue,    // GitHub issue/PR
    Rss,            // RSS/Atom/JSON Feed
}
```

**`AuthStrategy`** — Classification of authentication requirements (inspired by OpenCLI's `Strategy` enum):

```rust
pub enum AuthStrategy {
    NoAuth,   // PDF, CSV, RSS, public web
    ApiKey,   // GitHub token from user settings
    Cookie,   // Browser session (Phase 3: Twitter/X)
}
```

**`ExtractInput`**:

```rust
pub struct ExtractInput {
    pub source_type: SourceType,      // Explicit or Auto
    pub content: String,              // URL, raw text, or base64-encoded bytes
    pub encoding: Option<String>,     // "base64" or None (plain text)
    pub filename: Option<String>,     // For auto-detection and original file naming
    pub config: HashMap<String, String>, // API tokens, etc.
}
```

**`ExtractResult`**:

```rust
pub struct ExtractResult {
    pub text: String,                      // Clean Markdown output
    pub suggested_filename: String,        // e.g. "report.md"
    pub metadata: ExtractMetadata,         // Title, author, source URL, etc.
    pub original_content: Vec<u8>,         // Raw bytes of original file (for storage)
}

pub struct ExtractMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub source_url: Option<String>,
    pub language: Option<String>,
    pub page_count: Option<usize>,
}
```

### Registry Pattern (inspired by OpenCLI)

```rust
pub struct ExtractorRegistry {
    extractors: HashMap<SourceType, Arc<Box<dyn SourceExtractor>>>,
}

impl ExtractorRegistry {
    pub fn register(&mut self, extractor: Box<dyn SourceExtractor>) { ... }
    pub fn extract(&self, input: ExtractInput) -> Result<ExtractResult, ExtractError> { ... }
}
```

Automatic dispatch:
- `source_type: "pdf"` → routes to `PdfExtractor`
- `source_type: "auto"` → detects type from `filename` extension or magic bytes, then dispatches
- `source_type: "url"` → routes to `UrlExtractor` (backward compatible)
- `source_type: "text"` → routes to `TextExtractor` (backward compatible)

---

## Integration

### Modified Ingest Flow

```
                    POST /api/ingest
                         │
          ┌──────────────▼──────────────┐
          │ Parse source_type + encoding │
          └──────────────┬──────────────┘
                         │
          ┌──────────────▼──────────────┐
          │ ExtractorRegistry.extract() │
          │ • Auto-detect if "auto"     │
          │ • Decode base64 if needed   │
          │ • Dispatch to Extractor     │
          └──────────────┬──────────────┘
                         │
              ┌──────────▼──────────┐
              │  Extraction result? │
              └──────┬─────────┬────┘
                     │         │
               Success      Failure
                     │         │
          ┌──────────▼──┐  ┌──▼──────────────┐
          │ Write .md    │  │ Write original   │
          │ to sources/  │  │ to sources/      │
          │ + save orig  │  │ Return error msg │
          └──────────────┘  └──────────────────┘
```

### API Changes

`POST /api/ingest` — extended `IngestRequest`:

```json
{
  "source_type": "pdf | docx | url | text | auto | ...",
  "content": "<url, text, or base64>",
  "encoding": "base64",
  "filename": "report.pdf",
  "branch": "user/default"
}
```

- `encoding`: Optional. `"base64"` for binary files, omit for plain text/URL.
- `filename`: Optional. Used for auto-detection and original file preservation.
- All existing `source_type` values (`"url"`, `"text"`, `"file"`) remain valid.
- `"file"` is merged into `"auto"` (auto-detect from extension).

### File Storage

After successful extraction, `sources/` directory contains both:

```
sources/
├── report.pdf           ← Original binary (preserved, never modified)
├── report.md            ← Extracted Markdown (Compile input)
├── data.csv             ← Original CSV
├── data.csv.md          ← Extracted Markdown table
└── source-a1b2c3d4.md   ← URL/TEXT: extracted markdown
```

Rules:
- Original files are **always preserved**, never overwritten
- Extracted `.md` file is the clean Markdown output
- If extraction fails: original file is saved, `.md` is not written, error returned to user

### IngestResponse

```json
{
  "filename": "report.md",
  "content_hash": "a1b2c3...",
  "extracted": true,
  "extract_error": null,
  "original_filename": "report.pdf"
}
```

On failure:

```json
{
  "filename": "report.pdf",
  "content_hash": "d4e5f6...",
  "extracted": false,
  "extract_error": "PDF parsing failed: corrupted file header",
  "original_filename": null
}
```

---

## Phase 1 Extractors

### 1. TextExtractor

**Crate:** None (built-in)

**Logic:** Passthrough — normalize empty lines, trim trailing whitespace. Input is user-typed text, no encoding needed.

**Input:** Plain text string
**Output:** Same text, whitespace-normalized

```
Input:  "Hello\n\n\nWorld  "
Output: "Hello\n\nWorld"
```

---

### 2. UrlExtractor (Enhanced)

**Crate:** `reqwest` (existing) + `html2md`

**Logic:**
1. Fetch URL content via `reqwest`
2. Extract main article content using readability heuristics (strip nav, sidebar, footer, ads)
3. Convert HTML body to Markdown via `html2md`
4. Extract metadata: `<title>`, `<meta description>`, `<meta author>`

**Input:** URL string
**Output:** Clean Markdown article with metadata in frontmatter

```
Input:  "https://example.com/article"
Output: "---\ntitle: Article Title\nauthor: John\ndescription: ...\n---\n\n# Article Title\n\nContent in Markdown..."
```

**Key improvement over current fetch_url:** Current returns raw HTML. Extractor returns clean, LLM-ready Markdown.

---

### 3. MarkdownExtractor

**Crate:** None (validate-only)

**Logic:**
1. Validate: check for broken frontmatter, fix common issues
2. Normalize: ensure consistent heading levels, fix indentation
3. Passthrough valid Markdown unchanged

**Input:** Markdown string
**Output:** Validated and normalized Markdown

```
Input:  "---\ntitle: Test\n---\n\n# Hello\n  Indented code"
Output: "---\ntitle: Test\n---\n\n# Hello\n  Indented code"  (valid, passthrough)
```

---

### 4. CsvExtractor

**Crate:** `csv`

**Logic:**
1. Parse CSV rows via `csv` crate
2. First row as header
3. Generate Markdown table with aligned columns
4. Detect delimiter automatically (comma, tab, semicolon)

**Input:** CSV text (base64 or raw)
**Output:** Markdown table

```
Input:  "name,age,city\nAlice,30,NYC\nBob,25,SF"
Output: "| name  | age | city |\n|-------|-----|------|\n| Alice | 30  | NYC  |\n| Bob   | 25  | SF   |"
```

---

### 5. PdfExtractor

**Crate:** `pdf-extract`

**Logic:**
1. Decode base64 input to bytes
2. Parse PDF via `pdf-extract` crate (pure Rust, no system deps)
3. Extract text page by page
4. Insert page separators: `---` between pages
5. Preserve basic structure: detect headings by font size heuristics

**Input:** Base64-encoded PDF bytes
**Output:** Markdown with page breaks

```
Input:  <base64 PDF>
Output: "# Document Title\n\nPage 1 content...\n\n---\n\nPage 2 content..."
```

**Dependencies:** `pdf-extract` (pure Rust, no `libpoppler` or system libs needed)

**Limitations (acceptable for Phase 1):**
- No OCR for scanned PDFs (Phase 2: `leptess` + Tesseract)
- Complex multi-column layouts may lose reading order

---

### 6. DocxExtractor

**Crate:** `docx-rs`

**Logic** (inspired by MinerU's `DocxConverter`):
1. Decode base64 input to bytes
2. Parse OOXML package via `docx-rs`
3. Iterate paragraphs:
   - Detect headings by style (Heading 1-6) → `#` level headers
   - Convert lists (numbered + bullet)
   - Extract tables → Markdown table format
   - Inline formatting: **bold**, *italic*, `code`
4. Extract embedded images as base64 data-URIs (skip for Phase 1 if complex)
5. Generate YAML frontmatter with document metadata

**Input:** Base64-encoded DOCX bytes
**Output:** Structured Markdown

```
Input:  <base64 DOCX>
Output: "---\ntitle: Project Proposal\nauthor: Jane\n---\n\n# Introduction\n\nContent...\n\n## Background\n\nMore content..."
```

**Dependencies:** `docx-rs` (pure Rust OOXML reader)

**Key simplification vs MinerU:** No OMML formula conversion, no image extraction, no textbox handling — these are out of scope for wiki compilation.

---

### 7. PptxExtractor

**Crate:** `pptx-rs` or `undoc`

**Logic:**
1. Decode base64 input to bytes
2. Parse PPTX via crate
3. Extract slides → each slide becomes a `## Slide N` section
4. Extract text from text boxes, shapes, tables
5. Preserve slide order
6. Extract speaker notes (if available) as blockquotes

**Input:** Base64-encoded PPTX bytes
**Output:** Slide-structured Markdown

```
Input:  <base64 PPTX>
Output: "---\ntitle: Q4 Review\nslides: 12\n---\n\n## Slide 1\n\nTitle text\n\n- Bullet 1\n- Bullet 2\n\n## Slide 2\n..."
```

**Dependencies:** `pptx-rs` (or `undoc` if better support)

---

### 8. XlsxExtractor

**Crate:** `calamine`

**Logic:**
1. Decode base64 input to bytes
2. Parse XLSX via `calamine` (pure Rust Excel reader)
3. Iterate sheets → each sheet as a Markdown section
4. Each sheet → Markdown table (first row as header)
5. Skip empty sheets

**Input:** Base64-encoded XLSX bytes
**Output:** Sheet-structured Markdown tables

```
Input:  <base64 XLSX>
Output: "---\ntitle: Sales Data\nsheets: Q1, Q2\n---\n\n## Q1\n\n| Month | Revenue |\n|-------|--------|\n| Jan   | 10000  |\n\n## Q2\n\n| Month | Revenue |\n|-------|--------|\n| Apr   | 12000  |"
```

**Dependencies:** `calamine` (pure Rust, no Excel/Office dependency)

---

### 9. GitHubExtractor

**Crate:** `octocrab`

**Logic:**
1. Parse input URL/content to determine type (repo/issue/PR)
2. Authenticate via user's GitHub token from config (or anonymous if not configured)
3. **For repos:** Fetch README + directory structure + key files → Markdown
4. **For issues:** Fetch issue title + body + comments → Markdown thread
5. **For PRs:** Same as issue + fetch diff summary

**Input:** GitHub URL (repo, issue, PR)
**Output:** Structured Markdown

```
Input:  "https://github.com/wfnuser/cowiki/issues/31"
Output: "# cowiki-extractor: Support diverse source types\n\n**Author:** wfnuser\n**Status:** Open\n\n## Description\n\nExtend cowiki-extractor to support..."
```

**Dependencies:** `octocrab` (official GitHub API Rust client)

**Auth:** Reads `GITHUB_TOKEN` from user's settings (stored in DB). Falls back to anonymous (60 req/hr) if not configured.

---

### 10. RssExtractor

**Crate:** `feed-rs`

**Logic:**
1. Parse feed URL → fetch XML/JSON via `reqwest`
2. Parse via `feed-rs` (supports RSS 2.0, Atom, JSON Feed)
3. Extract feed metadata: title, description, link
4. For each entry: title, author, published date, content/summary, link
5. Output as structured Markdown list

**Input:** RSS/Atom/JSON Feed URL
**Output:** Markdown feed digest

```
Input:  "https://example.com/feed.xml"
Output: "---\ntitle: Example Blog\nentries: 10\n---\n\n## 2026-06-05 - Post Title\n\nSummary text... [Read more](https://...)\n\n## 2026-06-04 - Another Post..."
```

**Dependencies:** `feed-rs` (pure Rust, no system deps)

---

## Reference Projects: How They Handle Extraction

Before detailing Phase 2/3 extractors, here's how each reference project approaches the same problem:

### OpenCLI — "Browser as Client"

OpenCLI uses a **Chromium browser via CDP (Chrome DevTools Protocol)** as its network layer. All authenticated API calls happen inside `page.evaluate()` — meaning the browser's native `fetch()` is used, complete with cookies, TLS fingerprints, and User-Agent. This bypasses anti-bot detection because from the server's perspective, the request comes from a real browser.

| Service | Mechanism |
|---------|-----------|
| **Twitter/X** | Browser cookies (`auth_token` + `ct0`) + hardcoded `BEARER_TOKEN` (public, from Twitter's JS bundle) + internal GraphQL API at `/i/api/graphql/{queryId}/{Endpoint}` |
| **YouTube** | Watch page HTML → extract `ytInitialPlayerResponse` → find caption tracks → download srv3 XML → parse `[{start, dur, text}]` |
| **Reddit** | URL + `.json` = public API. No auth needed for reading public content. |
| **Apple Podcasts** | iTunes Search API (`itunes.apple.com/lookup`) — completely public, zero auth |

### MinerU — "Deep Format Parser"

MinerU parses each file format's internal binary structure (OOXML for DOCX/PPTX/XLSX, PDF dictionary objects) using format-specific converters. All converters produce a unified "Middle JSON", which is then rendered to Markdown by a shared output layer.

| Format | Parsing Strategy |
|--------|-----------------|
| **DOCX** | `python-docx` + `lxml` → iterate paragraphs by style → `BlockType.TITLE/TEXT/TABLE/EQUATION/IMAGE` |
| **PPTX/XLSX** | `convert_binary(file_bytes)` → internal `results[]` → `result_to_middle_json()` |
| **PDF** | `pypdfium2` render pages → classify (native text vs. scanned) → OCR via PaddleOCR or direct text extraction |
| **OCR** | Image → grayscale/threshold → DBNet text detection → CRNN text recognition → paragraph merging → reading-order sort |

### Open Notebook — "Unified content-core Library"

Open Notebook delegates all content extraction to a single Python library (`content-core`). The `extract_content()` function handles 50+ file formats, URL fetching, YouTube transcript extraction, and audio transcription — all behind one unified interface with engine/format/provider configuration. Extraction results feed into LangGraph workflows for further processing.

| Task | How |
|------|-----|
| **Document/URL extraction** | `content-core.extract_content(state)` with `document_engine="auto"`, `url_engine="auto"`, `output_format="markdown"` |
| **Audio transcription** | `audio_provider` + `audio_model` config → Whisper API or local model via Esperanto multi-provider interface |
| **Podcast creation** | `podcast-creator` library: LLM generates outline → LLM generates script → TTS synthesizes audio per speaker |
| **YouTube** | content-core internally handles YouTube URL → transcript extraction |

---

## Phase 2 Extractors

Phase 2 extractors require additional Rust crates with optional system dependencies, or external API access beyond simple HTTP fetch.

### 11. YouTubeExtractor

**Crate:** `reqwest` (no dedicated Rust crate; manual InnerTube API calls)

**Auth Strategy:** `NoAuth` — YouTube's InnerTube API works with browser-generated API keys scraped from page HTML, no user login required.

**Logic** (inspired by OpenCLI's `clis/youtube/transcript.js`):
1. Parse video URL → extract `videoId`
2. Fetch watch page HTML → extract `INNERTUBE_API_KEY` and `ytInitialPlayerResponse` via JSON string parsing
3. From player response, extract `captions.playerCaptionsTracklistRenderer.captionTracks[]`
4. Select track by language preference (default: first non-ASR track, fallback to ASR)
5. Fetch caption XML from `track.baseUrl` (append `&fmt=srv3` for structured format)
6. Parse XML segments → `[{start, dur, text}]`
7. Output: `## Title\n\n00:00 - Text\n00:05 - More text...`
8. Also fetch chapters (if available) from InnerTube `/next` API → insert as Markdown headings

**Key insight from OpenCLI:** YouTube transcript doesn't need OAuth or API keys. The `INNERTUBE_API_KEY` is embedded in every watch page HTML. The caption `baseUrl` works with a simple GET request. The `pot` (proof-of-origin token) parameter needed for json3 format is only required when accessing from outside the browser session — but the sr

v3 XML format works without it.

**Input:** YouTube URL
**Output:** Timestamped transcript Markdown

```
Input:  "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
Output: "---\ntitle: Video Title\nduration: 3:32\n---\n\n## Chapters\n\n## 00:00 - Introduction\n\nTranscript text...\n\n## 00:15 - Main Topic\n\nMore transcript..."
```

**Dependencies:** `reqwest` (already in project) + `quick-xml` or `roxmltree` for caption XML parsing.

**Limitations:**
- No captions available for some videos (author didn't enable + no auto-caption)
- Rate limiting: ~100 requests/IP/day for watch page scraping
- No OAuth-protected content (private/unlisted videos)

---

### 12. EpubExtractor

**Crate:** `epub` (pure Rust EPUB reader)

**Auth Strategy:** `NoAuth`

**Logic:**
1. Decode base64 input to bytes
2. Parse EPUB archive (ZIP container) via `epub` crate
3. Extract metadata: title, author, publisher, language, cover image
4. Iterate spine items (content documents in reading order):
   - Each XHTML/HTML document → strip tags → Markdown
   - Detect chapter boundaries from `<h1>`-`<h6>` tags
   - Preserve images as base64 data-URIs (or skip for Phase 2)
5. Generate YAML frontmatter + concatenated Markdown body
6. Output table of contents as leading Markdown list

**Input:** Base64-encoded EPUB bytes
**Output:** Structured Markdown with TOC

```
Input:  <base64 EPUB>
Output: "---\ntitle: Book Title\nauthor: Author Name\nchapters: 12\n---\n\n## Contents\n\n- Chapter 1\n- Chapter 2\n\n# Chapter 1\n\nContent..."
```

**Dependencies:** `epub` crate (pure Rust, reads EPUB = ZIP + XML + XHTML)

**Limitations:**
- DRM-protected EPUBs cannot be read
- Complex CSS-styled text may lose formatting
- Fixed-layout EPUBs (children's books) not well supported

---

### 13. RedditExtractor

**Crate:** `reqwest` (Reddit's `.json` API is the simplest of any major platform)

**Auth Strategy:** `NoAuth` for public content; optional OAuth2 for higher rate limits.

**Background — Why Reddit is trivial:**

Reddit is by far the most developer-friendly platform: **append `.json` to any Reddit URL and you get structured JSON back.** No API key, no OAuth, no browser session. OpenCLI uses `Strategy.COOKIE` for write operations only (posting, voting). For reading — which is all cowiki needs — anonymous access works perfectly.

```bash
# Any of these just works:
curl https://www.reddit.com/r/rust/hot.json
curl https://www.reddit.com/r/rust/comments/abc123.json
curl https://www.reddit.com/user/username/submitted.json
```

**Logic** (inspired by OpenCLI's `clis/reddit/`):
1. Parse input URL → determine type: subreddit / post / user / comment thread
2. Append `.json` to URL → `reqwest::get()` → JSON response
3. Subreddit hot feed: `GET /r/{subreddit}/hot.json?limit=25`
4. Post + comments: `GET /comments/{postId}.json` — returns `[{post}, {comments}]`
5. User posts: `GET /user/{username}/submitted.json`
6. Output Markdown:
   - Post: title + selftext + top N comments (sorted by score)
   - Subreddit: chronological/numbered list of posts with scores

**Input:** Reddit URL
**Output:** Markdown post/thread digest

```
Input:  "https://reddit.com/r/rust/comments/abc123"
Output: "---\ntitle: Post Title\nsubreddit: r/rust\nscore: 342\n---\n\n## Post\n\nContent...\n\n## Top Comments\n\n### u/commenter1 (↑120)\n\n..."
```

**Dependencies:** `reqwest` only. Reddit's JSON API is so simple that no special crate is needed.

**Auth:** Anonymous by default (60 req/min). Optional OAuth2 `client_credentials` grant (1000 req/10min) via `REDDIT_CLIENT_ID` + `REDDIT_CLIENT_SECRET` in user settings.

**Limitations:**
- Anonymous rate limit: 60 req/min (ample for wiki ingestion)
- NSFW content requires OAuth
- Comment tree depth: API returns max depth 10 by default

---

### 14. OcrExtractor (Images → Text)

**Crate:** `leptess` (Rust bindings to Tesseract/LibTesseract)

**Auth Strategy:** `NoAuth`

**Logic** (pattern reference: MinerU's `mineru/model/ocr/`):
1. Decode base64 input → image bytes
2. Load image via `image` crate → grayscale + thresholding (preprocessing)
3. Call `leptess` to run Tesseract OCR
4. Post-process: merge adjacent text blocks, detect paragraphs
5. Optional: use MinerU-style layout detection (but simplified) — split page into regions, read left-to-right, top-to-bottom
6. Output Markdown: `## Image OCR Result\n\nRecognized text...`

**Input:** Base64-encoded image (PNG, JPEG, TIFF, BMP)
**Output:** Extracted text as Markdown

```
Input:  <base64 screenshot.png>
Output: "---\ntype: ocr\nengine: tesseract\nlanguage: eng\n---\n\n## OCR Result\n\nExtracted text from image..."
```

**Dependencies:** `leptess` (tesseract-sys bindings) + `image` (image loading/preprocessing) + **system dependency: `libtesseract`** or `tesseract` CLI installed.

**Why system dependency matters:** This is the first extractor requiring an OS-level install. The `leptess` crate links to `libtesseract.so`/`libtesseract.dylib`/`tesseract.dll`. On Linux: `sudo dnf install tesseract tesseract-langpack-eng`.

**Limitations:**
- System dependency — extractor gracefully degrades if tesseract not installed (returns error with install instructions)
- Handwriting accuracy poor with standard tesseract models
- No layout analysis (text-only; MinerU-level layout detection is Phase 3+)
- Languages beyond English require additional `tesseract-langpack-*` packages

---

## Phase 3 Extractors

Phase 3 requires third-party API services or significant infrastructure. These extractors follow the same `SourceExtractor` trait but have external service dependencies.

### 15. TwitterExtractor

**Crate:** `reqwest` only (API calls to third-party service)

**Auth Strategy:** `ApiKey` — Cowiki calls a third-party API, not Twitter directly.

**Background — Why not DIY:**

OpenCLI uses `Strategy.COOKIE` (browser-based auth) for Twitter — the user logs in via a headful browser, and the system reuses browser cookies. This works for a local CLI but is **not feasible for a server-side Rust service**: cowiki has no browser, no CDP protocol, and per-user persistent browser sessions would be extremely expensive.

The issue explicitly states: "DIY scrapers break every 2-4 weeks due to fingerprinting + IP bans." Even OpenCLI's approach requires maintaining stealth browser profiles, rotating fingerprints, and constant updates to keep up with Twitter's anti-bot detection.

**Logic:**
1. User configures a third-party API provider in cowiki settings:
   - **ScrapeCreators** (twitterapi.io) — official API wrapper, ~$49/mo
   - **Apify** (apify.com/twitter) — actor-based scraping, pay-per-use
   - **SocialData** (socialdata.tools) — tweet/user lookup API
2. User sets `TWITTER_API_PROVIDER` and `TWITTER_API_KEY` in their personal settings
3. Extractor maps cowiki's `SourceType::Twitter` to the configured provider:
   - Tweet lookup: `GET {provider}/tweet/{id}` → title + text + media URLs + metrics
   - User timeline: `GET {provider}/timeline/{username}` → recent tweets
   - Thread: `GET {provider}/thread/{id}` → full thread with replies
4. Output Markdown:
   ```
   ---
   title: "Tweet by @user"
   date: 2026-06-06
   likes: 1234
   retweets: 567
   ---

   Tweet text content...

   ## Media
   - [Image 1](url)
   ```

**Input:** Tweet URL or Twitter handle
**Output:** Markdown tweet/thread digest

**Dependencies:** `reqwest` only. The complexity is **not in the code** — it's in the third-party API provider selection and cost.

**Auth:** User-provided API key for their chosen third-party service. Stored in DB user settings alongside GitHub token.

**Limitations:**
- **Costs money.** Cannot operate without a paid third-party API subscription.
- Rate limits depend on the chosen provider's plan
- Login-only content (private accounts) requires the third-party API to have its own auth mechanism
- Provider APIs have different response formats → adapter per provider

---

### 16. XiaohongshuExtractor (小红书)

**Crate:** `reqwest` only

**Auth Strategy:** `ApiKey` — Third-party API only. DIY extraction is not viable.

**Background — Why 小红书 is classified as "Very Hard":**

小红书's anti-scraping is among the most aggressive in the industry. No open-source project (including OpenCLI, which covers 166 services) has a working 小红书 implementation. The technical barriers are layered:

**Layer 1 — API Signature (`xs`, `xt`, `x-s`, `x-t` headers):**

Every API request to 小红书 requires cryptographic signature headers (`x-s` and `x-t`). These are computed by obfuscated JavaScript that runs in the mobile app or web client. The signature algorithm:
- Uses a timestamp + URL path + request body as input
- Involves multiple rounds of AES encryption and custom bit manipulation
- The JavaScript code is obfuscated, minified, and **changes approximately monthly** with each app update
- Reverse-engineering requires decompiling the APK → extracting the native `.so` library → finding the signature function in obfuscated JNI code

**Layer 2 — TLS Fingerprinting:**

小红书's CDN edge performs JA3/JA4 TLS fingerprinting. Requests from `reqwest` (Rustls/OpenSSL) or `curl` are rejected at the TLS handshake level before any HTTP data is exchanged. Only browser-like TLS stacks (Chrome's BoringSSL with correct cipher suite ordering) pass through.

**Layer 3 — Device Registration:**

Most API endpoints require a `device_id` and `device_fingerprint` that are registered with 小红书's device attestation service. This involves:
- Hardware-level fingerprinting (screen resolution, CPU cores, GPU vendor, OS build number)
- A registration handshake that proves the device is "real" (not an emulator)
- Rate limiting per device: ~10 requests before captcha triggers for unregistered devices

**Layer 4 — Captcha Walls:**

After ~10 requests without proper device registration, 小红书 deploys a captcha wall:
- Light mode: Slider captcha (drag a puzzle piece)
- Heavy mode: Cloudflare Turnstile or custom captcha requiring interactive solving
- Neither can be bypassed programmatically without captcha-solving services

**Why OpenCLI can't do it either:**

OpenCLI's architecture (browser CDP) could theoretically solve Layers 2 and 4 (TLS + captcha) — but Layer 1 (API signature) and Layer 3 (device registration) would still require reverse-engineering the mobile app's native code, which is beyond the scope of a browser-based CLI. The signature algorithm changes monthly, requiring constant maintenance.

**Third-party API providers (the only viable path):**

| Provider | Coverage | Pricing Model |
|----------|----------|---------------|
| **Apify** (apify.com/xiaohongshu) | Notes, users, search | Pay-per-result or monthly subscription |
| **ScrapeCreators** (scrapecreators.com) | Notes, profiles, comments | Monthly subscription ~$79+/mo |
| **Bright Data** (brightdata.com) | Web scraping infrastructure with 小红书 collector | Pay-per-request |
| **Oxylabs** (oxylabs.io) | Social media scraping API | Enterprise pricing |

**Logic:**
1. User selects a third-party API provider and configures credentials in cowiki settings
2. Set `XHS_API_PROVIDER` (e.g., `"apify"`) and `XHS_API_KEY` in user's personal settings
3. Extractor implements a lightweight **adapter pattern** — one adapter per provider:

```rust
trait XhsApiAdapter {
    async fn get_note(&self, note_id: &str) -> Result<XhsNote, ExtractError>;
    async fn get_user(&self, user_id: &str) -> Result<XhsUser, ExtractError>;
}

// Provider-specific adapters, each ~50 lines:
struct ApifyXhsAdapter { api_key: String }
struct ScrapeCreatorsXhsAdapter { api_key: String }
// Easy to add new providers
```

4. Output Markdown (unified format regardless of provider):
   ```
   ---
   title: "小红书笔记标题"
   author: 作者名
   likes: 2345
   collected: 890
   ---

   笔记正文内容...

   ## 图片
   - [图片 1](url)
   ```

**Dependencies:** `reqwest` only. The complexity is in the third-party API provider integration, not in the Rust code.

**Auth:** User-provided API key for their chosen third-party service. Stored in DB alongside other user tokens.

**Cost comparison with Twitter:**

小红书 data is typically **2-3x more expensive** than Twitter data via third-party APIs, due to:
- Higher scraping difficulty (mobile app reverse-engineering vs. web-only)
- Device farm costs (providers must maintain pools of registered Android devices)
- More frequent signature algorithm updates requiring dedicated engineering

**Limitations:**
- **Must pay.** No free tier exists for 小红书 data extraction.
- Provider APIs have different response formats → adapter code per provider
- Login-only content (private notes, DMs) generally not available even via third-party APIs
- Some providers watermark images with their branding
- Content velocity: trending notes may be available with delay (minutes to hours)

---

### 17. PodcastExtractor (Audio → Text)

**Crate:** `reqwest` + user-provided transcription backend

**Auth Strategy:** `NoAuth` or `ApiKey` depending on transcription backend.

**Logic** (inspired by OpenCLI's `apple-podcasts/episodes.js` for feed parsing + Whisper API for transcription):
1. Input: podcast RSS feed URL or episode audio URL
2. **Feed parsing** (same pattern as `RssExtractor`):
   - Parse RSS → extract episode list with audio URLs
   - If input is an audio URL, skip this step
3. **Transcription** — two options:
   - **Whisper API** (OpenAI): POST audio URL to `/v1/audio/transcriptions` — pay per minute
   - **Local Whisper** (`whisper-rs` crate): Run Whisper.cpp locally via Rust bindings — free but requires model download (1.5GB for `medium`, 2.9GB for `large-v3`)
4. Output Markdown:
   ```
   ---
   title: Episode Title
   podcast: Podcast Name
   date: 2026-06-06
   duration: 45:32
   ---

   ## Transcript

   Speaker 1: Welcome to the show...

   Speaker 2: Today we're discussing...
   ```

**Dependencies:** `reqwest` + optionally `whisper-rs` (Rust bindings to `whisper.cpp`, with system deps: `cmake`, C++ compiler, model files on disk).

**Auth:** If using OpenAI Whisper API → user's `OPENAI_API_KEY` (already in config). Local Whisper → no auth needed.

**Design Decision — Local vs API transcription:**

| Factor | Whisper API (OpenAI) | Local Whisper.cpp |
|--------|---------------------|-------------------|
| Cost | ~$0.006/min | Free |
| Setup | Zero (existing API key) | Model download + C++ build chain |
| Quality | Excellent (large-v3) | Depends on model size |
| Latency | 1-2x audio duration | 1-3x audio duration on GPU, 3-10x on CPU |

**Recommendation:** Default to OpenAI Whisper API (already in project's dependency chain). Local `whisper-rs` as optional feature flag (`--features local-whisper`).

**Limitations:**
- 1-hour podcast = ~$0.36 via Whisper API
- Local Whisper requires significant disk space and RAM
- Speaker diarization (who said what) not supported out of the box
- Non-English languages have varying accuracy (good for major languages, poor for low-resource ones)

---

## Auth Strategy Architecture (Phase 2/3 additions)

Phase 2/3 extractors introduce a broader auth spectrum, extending Phase 1's simple model:

```
NoAuth ───── PDF, CSV, Markdown, RSS, YouTube, EPUB
ApiKey ───── GitHub, Reddit (OAuth2)
ApiKey ───── Twitter/X, 小红书 (third-party API)
ApiKey ───── Podcast (Whisper API key, or local)
SystemDep ── OCR (libtesseract)
Cookie ───── (future: browser-based auth if cowiki ever gets headless browser support)
```

All auth configuration is stored per-user in the DB (`api_keys` / `user_settings` table). Extractor code reads config from `ExtractInput.config: HashMap<String, String>` populated at request time.

---

## Implementation Plan

### Phase 1 (current issue) — 10 Pure Rust extractors

| Step | Task | Effort |
|------|------|--------|
| 1 | Create `crates/extractor/` skeleton: Cargo.toml, lib.rs, error.rs, types.rs, registry.rs | 1d |
| 2 | Implement `TextExtractor` + `UrlExtractor` + `MarkdownExtractor` | 0.5d |
| 3 | Implement `CsvExtractor` | 0.5d |
| 4 | Implement `PdfExtractor` | 1d |
| 5 | Implement `DocxExtractor` | 1d |
| 6 | Implement `PptxExtractor` | 1d |
| 7 | Implement `XlsxExtractor` | 1d |
| 8 | Implement `GitHubExtractor` | 1d |
| 9 | Implement `RssExtractor` | 0.5d |
| 10 | Integrate into `routes/ingest.rs` (extend `do_ingest`) | 1d |
| 11 | Update `AppState` + `main.rs` to init `ExtractorRegistry` | 0.5d |
| 12 | Add `encoding` field to `IngestRequest` | 0.5d |
| 13 | Tests + end-to-end validation | 1d |

**Total estimate: ~10 working days**

### Out of Scope

- Compile pipeline changes (reads existing `sources/` unchanged)
- `re-extract` endpoint (future feature)
- Phase 2 extractors (YouTube, EPUB, Reddit, OCR)
- Phase 3 extractors (Twitter/X, 小红书, Podcast)

---

## Design Decisions Summary

| # | Decision | Choice | Rationale |
|---|----------|--------|-----------|
| 1 | Extract output storage | `sources/` — original + `.md` side-by-side | Zero change to Compile; both files in Git |
| 2 | Binary file transfer | Base64 + `encoding` field | Simple, backward-compatible JSON API |
| 3 | Type determination | Explicit + `"auto"` (by extension/magic bytes) | Precise control for agents, convenience for humans |
| 4 | GitHub auth | Per-user token (stored in DB user settings) | Avoid rate-limit sharing; anonymous fallback |
| 5 | API backward compat | Extend `IngestRequest` fields | `"url"`/`"text"` unchanged; new types via registry |
| 6 | Config storage | Extractor config in `cowiki.conf`; user tokens in DB | Separate system config from user secrets |
| 7 | Metadata | YAML frontmatter in extracted Markdown | Human-readable, Git-tracked, no extra DB table |
| 8 | File duplication | Always write both (original + extracted) | Simple rule, no comparison overhead |
| 9 | Error handling | Save original, return error + `extracted: false` | Preserve user input, actionable error message |
| 10 | Rust crates | Per Issue recommendations | Most mature, pure Rust, no system deps |
| 11 | `"file"` type | Merged into `"auto"` | Avoid type fragmentation |
| 12 | Re-extract | Not in Phase 1 | User can re-upload; revisit later |

---

## References

- [OpenCLI](https://github.com/jackwener/opencli) — Registry pattern, auth strategies (Strategy.COOKIE/PUBLIC), browser-based extraction (Twitter GraphQL, YouTube InnerTube, Reddit `.json`)
- [MinerU](https://github.com/opendatalab/MinerU) — Document parsing pipeline (DOCX/PPTX/XLSX/PDF → Middle JSON → Markdown), OCR workflow (DBNet + CRNN + paragraph merging)
- [Open Notebook](https://github.com/lfnovo/open-notebook) — Unified content extraction via `content-core` library (50+ formats), Whisper API audio transcription, LangGraph workflow orchestration
- [CONTEXT.md](/CONTEXT.md) — Project domain glossary
- [ADR-0001](/docs/adr/0001-git-as-storage-backend.md) — Git as storage backend
