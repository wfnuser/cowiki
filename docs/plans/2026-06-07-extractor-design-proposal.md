# Design Proposal: Multi-Format Source Extraction for Cowiki

**Issue:** [#31](https://github.com/wfnuser/cowiki/issues/31)
**Status:** Proposed | **Date:** 2026-06-07

## 1. Problem Statement

Cowiki currently supports three source types at ingest time: `url` (raw HTML fetch), `text` (passthrough), and `file` (passthrough). Raw HTML is noisy for LLM compilation, binary documents are rejected outright, and structured formats like CSV or RSS feeds require manual preprocessing.

This proposal introduces a **pluggable extraction framework** — `crates/extractor/` — that converts 13+ diverse source types into clean, structured Markdown. The design is grounded in analysis of three mature open-source projects in the `third-party/` directory: **Open Notebook** (content-core library), **OpenCLI** (166-site CLI toolkit), and **MinerU** (document parsing engine).

## 2. Architecture

### 2.1 New Crate Structure

```
crates/extractor/src/
├── lib.rs              # SourceExtractor trait, create_default_registry()
├── error.rs            # ExtractError enum (7 variants)
├── types.rs            # SourceType(13), AuthStrategy, ExtractInput, ExtractResult
├── registry.rs         # ExtractorRegistry — HashMap<SourceType, Arc<dyn SourceExtractor>>
├── universal.rs        # content-core subprocess wrapper (PDF, DOCX, PPTX, EPUB, YouTube, OCR)
├── text.rs             # TextExtractor — passthrough
├── markdown.rs         # MarkdownExtractor — validation + normalization
├── csv.rs              # CsvExtractor — CSV → GFM table
├── xlsx.rs             # XlsxExtractor — calamine → multi-sheet Markdown tables
├── url.rs              # UrlExtractor — content-core → fallback scraper + html2md
├── github.rs           # GitHubExtractor — octocrab → README + issues
├── rss.rs              # RssExtractor — feed-rs → Markdown feed digest
└── (future)
    reddit.rs, twitter.rs, xiaohongshu.rs, podcast.rs
```

### 2.2 Core Trait

```rust
#[async_trait]
pub trait SourceExtractor: Send + Sync {
    fn supported_types(&self) -> Vec<SourceType>;
    fn auth_strategy(&self) -> AuthStrategy;
    async fn extract(&self, input: ExtractInput) -> Result<ExtractResult, ExtractError>;
}
```

### 2.3 Registry Dispatch

```rust
pub struct ExtractorRegistry {
    extractors: HashMap<SourceType, Arc<dyn SourceExtractor>>,
}
```

Each extractor registers for one or more `SourceType` values. The registry dispatches by type:

- `SourceType::Pdf` → `PdfExtractor`
- `SourceType::Auto` → detects type from filename extension, then dispatches
- `SourceType::Url` → `UrlExtractor` (backward-compatible with existing API)

### 2.4 Key Types

```rust
pub enum SourceType {
    Auto, Text, Url, Pdf, Docx, Pptx, Xlsx, Csv, Markdown,
    GitHubRepo, GitHubIssue, Rss,
    // Phase 2: YouTube, Epub, Reddit, Ocr
    // Phase 3: Twitter, Xiaohongshu, Podcast
}

pub enum AuthStrategy { NoAuth, ApiKey, Cookie }

pub struct ExtractInput {
    pub source_type: SourceType,
    pub content: String,            // URL, raw text, or base64-encoded bytes
    pub encoding: Option<String>,   // "base64" for binary files
    pub filename: Option<String>,   // for auto-detection and original file naming
    pub config: HashMap<String, String>,
}

pub struct ExtractResult {
    pub text: String,               // Clean Markdown
    pub suggested_filename: String,
    pub original_content: Vec<u8>,  // Raw original bytes (always preserved)
    pub metadata: ExtractMetadata,
}
```

## 3. API Integration

### 3.1 Ingest Flow

```
POST /api/ingest { source_type, content, encoding?, filename? }
       │
       ▼
ExtractorRegistry.extract(ExtractInput)
       │
       ├── Success → Write original (.pdf/.docx) + extracted (.md) to sources/
       └── Failure → Save original, return { extracted: false, extract_error: "..." }
```

### 3.2 API Changes (Backward-Compatible)

New optional fields on `IngestRequest`:

| Field | Type | Purpose |
|-------|------|---------|
| `encoding` | `Option<String>` | `"base64"` for binary files; omit for plain text/URL |
| `filename` | `Option<String>` | Enables auto-detection and preserves original file |

Existing `"url"` and `"text"` values for `source_type` continue working unchanged. New values (`"pdf"`, `"docx"`, etc.) route through the same `ExtractorRegistry`.

### 3.3 File Storage

```
sources/
├── report.pdf          ← Original binary (always preserved, never modified)
├── report.md           ← Extracted Markdown (Compile input)
├── data.csv            ← Original CSV
├── data.csv.md         ← Extracted Markdown table
```

### 3.4 Response Format

Success:
```json
{ "filename": "report.md", "content_hash": "a1b2...", "extracted": true, "extract_error": null }
```

Failure:
```json
{ "filename": "report.pdf", "content_hash": "d4e5...", "extracted": false, "extract_error": "PDF parsing failed: corrupted header" }
```

## 4. Extractor Designs

### 4.1 Guiding Principle

The `third-party/` directory contains three production-grade open-source projects. We analyzed each to identify proven approaches and avoid reinventing wheels:

| Project | Language | What It Does | What We Learned |
|---------|----------|-------------|-----------------|
| **Open Notebook** | Python | AI research assistant — ingests PDF, audio, video, URLs, produces notes/search/podcasts | Its `content-core` library provides a single `extract_content()` API for 50+ formats. We can call this as a subprocess rather than reimplementing each format. |
| **OpenCLI** | TypeScript | Turns 166 websites into CLI tools via browser CDP | Validates which web APIs work without authentication (YouTube InnerTube, Reddit `.json`), which require browser sessions (Twitter), and which are impossible even with a browser (Xiaohongshu — the only major platform absent from the 166-service catalog). |
| **MinerU** | Python | High-precision document parser for LLM consumption | Deep OOXML parsing (DOCX via 3,586-line `DocxConverter`), OCR pipeline, table/formula extraction. Demonstrates the enormous effort needed for production-quality document parsing — effort we avoid by delegating to `content-core`. |

---

### 4.2 Phase 1 Extractors

#### 4.2.1 PDF Extractor

**Parsing approach:** Delegate to `content-core` (the extraction engine behind Open Notebook).

**How content-core parses PDF:** It uses a chain of engines that cascade by capability — Docling (IBM's document understanding library) is tried first for best fidelity, then enhanced PyMuPDF for text extraction with table detection, and finally a simple text fallback. For scanned documents, it applies built-in OCR. Tables are converted to Markdown tables, mathematical formulas to LaTeX, and embedded images to base64 data URIs.

**Why delegate rather than implement in Rust:** The Rust crate `pdf-extract` handles plain text-based PDFs but cannot process scanned documents, complex multi-column layouts, tables, or formulas. Reaching MinerU-level PDF parsing quality would require implementing multiple ML models (layout detection, OCR, table structure recognition) — easily thousands of lines of Rust and ongoing model maintenance. `content-core` provides this for free with a single `extract_content()` call.

**Third-party references:**

| Project | What We Borrowed | Rationale |
|---------|-----------------|-----------|
| **Open Notebook** | `content-core` as primary engine | Open Notebook's `source.py:78` calls `await extract_content(content_state)` — the same single-call pattern we use. It configures `document_engine="auto"` and `output_format="markdown"`, producing exactly the output we need. |
| **MinerU** | Pipeline architecture reference (not code) | MinerU's `pipeline_analyze.py` demonstrates the ideal PDF extraction flow: classify (native vs. scanned) → extract/OCR → post-process (merge paragraphs, sort reading order). We get this same flow through `content-core` without implementing it ourselves. |

**Integration:** Python subprocess. If `content-core` is not installed, return an error with installation instructions (`pip install content-core`).

**Input:** Base64-encoded PDF bytes
**Output:** Markdown with page separators, headings, tables, and embedded images

#### 4.2.2 DOCX Extractor

**Parsing approach:** Delegate to `content-core`.

**How content-core parses DOCX:** It uses Docling to parse the OOXML XML tree. This involves iterating the document body's XML elements, identifying paragraph styles (Heading 1-6 → heading levels, list styles → bullet/numbered items), converting tables to Markdown/HTML via mammoth, transforming OMML equations to LaTeX, extracting embedded images from the ZIP package's `word/media/` folder and serializing them as base64 data URIs, and resolving style inheritance chains from `word/styles.xml`.

**Why delegate rather than implement in Rust:** MinerU's `DocxConverter` alone is 3,586 lines of Python — it handles XML namespaces, style inheritance, list numbering counters, OMML formula conversion, table alignment with mammoth, image extraction with format detection (WMF/EMF vector placeholder rendering), and edge cases like merged cells and structured document tags. Porting even a fraction of this to Rust is prohibitive. The `docx-rs` Rust crate provides only basic paragraph reading without style resolution, table parsing, or image extraction.

**Third-party references:**

| Project | What We Borrowed | Rationale |
|---------|-----------------|-----------|
| **Open Notebook** | `content-core` as primary engine | Same `extract_content()` API for all document formats. Content-core's Docling backend handles style resolution tables and image extraction — features we verified MinerU also does but don't need to implement. |
| **MinerU** | OOXML parsing reference (not ported) | MinerU's `docx_converter.py` proves what a complete DOCX parser looks like. Its `_walk_linear()` method iterates OOXML body children with dedicated handlers for each element type (`tbl` for tables, `p` for paragraphs, `sdt` for structured document tags, drawing elements for images). This validates that our delegation approach avoids thousands of lines of Rust. |

**Input:** Base64-encoded DOCX bytes
**Output:** Structured Markdown with headings, lists, tables, and embedded images

#### 4.2.3 PPTX Extractor

**Parsing approach:** Delegate to `content-core`.

**How content-core parses PPTX:** PPTX is also an OOXML ZIP package. Content-core's Docling backend opens the ZIP, locates slide files at `ppt/slides/slideN.xml`, extracts text from all shape types (text boxes, tables, group shapes, SmartArt), applies xy-cut sorting to determine reading order from shape coordinates, extracts speaker notes from `ppt/notesSlides/notesSlideN.xml`, and serializes embedded images from `ppt/media/` as base64 data URIs.

**Why delegate rather than implement in Rust:** PPTX text can be scattered across fundamentally different XML structures — `<a:p>` paragraphs in text boxes, `<a:tc>` cells in tables, nested group shapes, and SmartArt diagrams with their own coordinate systems. MinerU's `PptxConverter` handles each shape type with dedicated logic and applies coordinate-based sorting (`xycut_pp_sorter.py`) to reconstruct human reading order. There is no mature Rust PPTX parsing library.

**Third-party references:**

| Project | What We Borrowed | Rationale |
|---------|-----------------|-----------|
| **Open Notebook** | `content-core` as primary engine | Handles all shape types and reading order sorting automatically. Open Notebook stores extracted content as `source.full_text` in SurrealDB — the same "extracted Markdown" concept we use. |
| **MinerU** | PPTX parsing reference (not ported) | MinerU's `pptx_converter.py` demonstrates shape-type dispatch (text box vs. table vs. group vs. picture) and xy-cut sorting. Its `extract_slide_content()` handles each `MSO_SHAPE_TYPE` explicitly. Content-core provides equivalent functionality without us implementing these cases. |

**Input:** Base64-encoded PPTX bytes
**Output:** Slide-structured Markdown (`## Slide N`), with text, tables, and images

#### 4.2.4 Markdown Extractor

**Parsing approach:** Pure Rust — no external engine needed.

**Logic:** Parse YAML frontmatter to extract metadata (title, author, source). Validate frontmatter syntax. Normalize whitespace: collapse 3+ blank lines to 2, trim trailing whitespace on each line. Pass through valid Markdown unchanged.

**Third-party references:** None — Markdown is trivial to handle.

**Input:** Markdown string (raw or base64-encoded)
**Output:** Validated and normalized Markdown with extracted frontmatter metadata

#### 4.2.5 CSV Extractor

**Parsing approach:** Pure Rust via `csv` crate.

**Logic:** Auto-detect delimiter by counting occurrences of likely delimiters (comma, tab, semicolon, pipe) in the first line. Treat the first row as column headers. Iterate remaining rows, escaping pipe characters in cell values. Produce a GFM-compliant Markdown table with header separator.

**Third-party references:** None — CSV is a simple delimited format.

**Input:** CSV text (raw or base64-encoded)
**Output:** GFM Markdown table

#### 4.2.6 XLSX Extractor

**Parsing approach:** Pure Rust via `calamine` crate.

**Logic:** Open the XLSX file via `calamine::open_workbook_from_rs()`. Iterate all sheet names. For each sheet, read the data range into rows. Determine column count from the widest row. Emit a Markdown section per sheet, with the first row as the table header and subsequent rows as data. Empty cells and error values are handled gracefully.

**Why pure Rust for XLSX but not for PPTX/DOCX:** XLSX is fundamentally a grid of cells — calamine reads this perfectly with zero loss. The complexity in XLSX is not in parsing (unlike DOCX with its style inheritance and OMML formulas) but in data representation, which is straightforward.

**Third-party references:**

| Project | What We Borrowed | Rationale |
|---------|-----------------|-----------|
| **MinerU** | Validated calamine-based approach | MinerU's `xlsx_converter.py` uses the same `calamine` library (Python equivalent) for multi-sheet iteration and cell extraction. This confirms calamine as the correct tool for the job. |

**Input:** Base64-encoded XLSX bytes
**Output:** Multi-sheet Markdown with GFM tables per sheet

#### 4.2.7 URL Extractor

**Parsing approach:** Two-tier — content-core as primary, Rust-native `scraper` + `html2md` as fallback.

**Tier 1 (content-core):** Best extraction quality. Content-core handles JavaScript-rendered pages via headless browser fallback, extracts main article content using readability algorithms, and converts to clean Markdown with embedded images as base64 data URIs.

**Tier 2 (Rust fallback):** Parse HTML DOM via `scraper` crate. Score each block-level element (`<div>`, `<section>`, `<article>`, `<main>`) by text length minus a link-density penalty (too many links relative to text suggests navigation, not content). Also check semantic selectors (`<article>`, `<main>`, `[role=main]`, `.content`, `.post-content`, `.article-content`). Extract the highest-scoring element's inner HTML and convert to Markdown via `html2md`. Extract `<title>` for metadata.

**Why two tiers:** Content-core produces the best output but requires Python. The Rust fallback ensures basic functionality without external dependencies. This is the same pattern used by content-core itself (engine chain: Docling → PyMuPDF → simple).

**Third-party references:**

| Project | What We Borrowed | Rationale |
|---------|-----------------|-----------|
| **Open Notebook** | `content-core` as primary URL engine | Open Notebook's `source.py` passes URL content through the same `extract_content()` pipeline as document files. Content-core's `url_engine="auto"` handles page fetching and content extraction with the same quality as its document engine. |
| **MinerU** | Not applicable | MinerU is a document parser; it does not handle web URLs. |

**Input:** Web page URL
**Output:** Clean Markdown article with title, metadata, and embedded images

#### 4.2.8 GitHub Extractor

**Parsing approach:** Pure Rust via `octocrab` crate (official GitHub API client).

**Logic:** Parse the input URL to determine the resource type — repo, issue, or PR. For repos: call `GET /repos/{owner}/{repo}` for metadata (stars, language, description), `GET /repos/{owner}/{repo}/readme` for the README (base64-decode the content), and `GET /repos/{owner}/{repo}/git/trees/main` for a top-level directory listing. For issues: call `GET /repos/{owner}/{repo}/issues/{number}` for the issue body, then `GET /repos/{owner}/{repo}/issues/{number}/comments` for the comment thread.

**Auth:** Anonymous mode (60 requests/hour — sufficient for public repos). Optional personal access token from user settings for higher rate limits.

**Third-party references:**

| Project | What We Borrowed | Rationale |
|---------|-----------------|-----------|
| **OpenCLI** | Validated read-only access pattern | OpenCLI's `clis/github/auth.js` uses browser-cookie auth for write operations (posting, starring). For reading — which is all cowiki needs — OpenCLI confirms that the GitHub REST API works with anonymous access. The `feed.js` and `comments.js` CLIs demonstrate the same issue/PR/repo reading patterns we use. |

**Input:** GitHub repository or issue URL
**Output:** Structured Markdown with README, directory tree, or issue body with comments

#### 4.2.9 RSS Extractor

**Parsing approach:** Pure Rust via `feed-rs` crate.

**Logic:** Fetch the feed URL via `reqwest`. Parse the response body through `feed-rs::parser::parse()`, which auto-detects RSS 2.0, Atom, and JSON Feed formats. Extract feed-level metadata (title, description). For each entry, extract title, published date, summary/content, and link. Output as a Markdown feed digest with entry headings, dates, and truncated summaries with "Read more" links.

**Third-party references:** None — RSS is a well-defined XML format with mature parsers in every language.

**Input:** RSS/Atom/JSON Feed URL
**Output:** Markdown feed digest with dated entry list

---

### 4.3 Phase 2 Extractors

#### 4.3.1 YouTube Extractor

**Parsing approach:** Delegate to `content-core`.

**How content-core extracts YouTube transcripts:** It fetches the watch page HTML, extracts the `ytInitialPlayerResponse` JSON blob, finds available caption tracks in `captions.playerCaptionsTracklistRenderer.captionTracks[]`, selects the best language match, downloads the caption track XML (srv3 format), and parses `[{start, dur, text}]` segments. Chapters are optionally extracted from the InnerTube `/next` API response.

**Why this works without an API key:** OpenCLI discovered — and content-core validates — that YouTube's InnerTube API key is embedded in every watch page's HTML. The `INNERTUBE_API_KEY` and the caption track `baseUrl` are public. No OAuth, no API key registration, no browser login is needed for public videos.

**Third-party references:**

| Project | What We Borrowed | Rationale |
|---------|-----------------|-----------|
| **OpenCLI** | Validated that InnerTube API requires no key | OpenCLI's `clis/youtube/transcript.js` (656 lines) is the definitive reference: it opens the watch page, finds the player captions module, intercepts timedtext URL fetch/XHR calls, and parses srv3 XML. The key insight — "no API key needed" — was proven here first. |
| **Open Notebook** | `content-core` wraps the same approach | Content-core's youtube-transcript integration provides a Python API for this exact InnerTube method. Using it avoids reimplementing the XML parsing and language selection logic in Rust. |

**Input:** YouTube video URL
**Output:** Timestamped transcript Markdown

#### 4.3.2 EPUB Extractor

**Parsing approach:** Delegate to `content-core`.

**Third-party references:**

| Project | What We Borrowed | Rationale |
|---------|-----------------|-----------|
| **Open Notebook** | `content-core` handles EPUB natively | EPUB is ZIP + XHTML — content-core extracts it as part of its 50+ format support. |

#### 4.3.3 Reddit Extractor

**Parsing approach:** Pure Rust via `reqwest` — the simplest platform API.

**Logic:** Append `.json` to any Reddit URL and GET it. For subreddits: `GET /r/{name}/hot.json?limit=25` returns a JSON array of posts with title, score, author, selftext, and comment count. For posts: `GET /comments/{postId}.json` returns `[{post}, {comments}]` with the full comment tree. For users: `GET /user/{username}/submitted.json`. Parse the JSON response and format as Markdown with titles, scores, authors, and content. No OAuth, no API key, no browser session needed for public content.

**Why Reddit is the easiest platform API:** OpenCLI discovered that every Reddit URL returns structured JSON simply by appending `.json`. Anonymous rate limit is 60 req/min — ample for wiki ingestion. Optional OAuth2 client credentials grant can raise this to 1000 req/10min.

**Third-party references:**

| Project | What We Borrowed | Rationale |
|---------|-----------------|-----------|
| **OpenCLI** | Validated `.json` endpoint and anonymous access | OpenCLI's `clis/reddit/read.js`, `hot.js`, and `subreddit.js` all use `fetch('/r/{name}/hot.json', {credentials: 'include'})` — the same pattern we use without the browser cookies. The `.json` endpoint discovery is the key finding. |

**Input:** Reddit post or subreddit URL
**Output:** Markdown post with top comments, or subreddit hot feed

#### 4.3.4 OCR Extractor (Image → Text)

**Parsing approach:** Delegate to `content-core`.

**Why delegate:** OCR is the most complex extraction task. It requires image preprocessing (grayscale conversion, thresholding, deskewing), text detection (locating text regions via DBNet or similar neural network), text recognition (CRNN or transformer-based model), and postprocessing (paragraph merging, reading-order sorting). MinerU implements this with PaddleOCR (PaddlePaddle + PyTorch models, hundreds of Python files). Content-core provides the same result through a single API call.

**Third-party references:**

| Project | What We Borrowed | Rationale |
|---------|-----------------|-----------|
| **MinerU** | OCR pipeline reference (not ported) | MinerU's `model/ocr/pytorch_paddle.py` and `model/utils/tools/infer/predict_system.py` implement the full detection→recognition→postprocessing pipeline with PaddleOCR. This validates the complexity of OCR and justifies our delegation approach. |
| **Open Notebook** | `content-core` as OCR engine | Content-core provides OCR through its document engine chain without requiring us to manage ML models or system dependencies. |

**Input:** Base64-encoded image (PNG, JPEG, TIFF, BMP)
**Output:** Extracted text as Markdown

---

### 4.4 Phase 3 Extractors

#### 4.4.1 Twitter/X Extractor

**Parsing approach:** Third-party API adapter only. No DIY scraping.

**Why DIY is infeasible:** OpenCLI achieves Twitter extraction through `Strategy.COOKIE` — the user logs into Twitter in a real browser, and the system reads browser cookies (`auth_token`, `ct0`) via CDP, then calls Twitter's internal GraphQL API (`/i/api/graphql/{queryId}/{Endpoint}`) from inside the browser. A hardcoded public `BEARER_TOKEN` (from Twitter's web client JS bundle) is combined with the browser cookie session.

A server-side Rust service cannot maintain persistent browser sessions per user, cannot provide Chrome's TLS fingerprint, and cannot execute JavaScript to dynamically resolve GraphQL query IDs that change with each Twitter web client deploy. Even OpenCLI's approach requires ongoing maintenance as query IDs rotate.

**Third-party references:**

| Project | What We Borrowed | Rationale |
|---------|-----------------|-----------|
| **OpenCLI** | Validated the DIY infeasibility for server-side use | `clis/twitter/timeline.js` (212 lines) and `clis/twitter/utils.js` show the exact mechanism: hardcoded `TWITTER_BEARER_TOKEN` + browser cookies + dynamic GraphQL query IDs + `page.evaluate(fetch(...))` from inside the browser. `clis/twitter/auth.js` confirms `Strategy.COOKIE` with persistent browser sessions. All three depend on browser infrastructure unavailable to a Rust backend. |
| **MinerU, Open Notebook** | Not applicable | Neither project handles social media APIs. |

**Implementation:** Lightweight adapter layer. User selects a third-party API provider (ScrapeCreators, Apify, or SocialData) and configures their API key. The extractor forwards requests to the chosen provider's REST API and normalizes the response to Markdown. One adapter per provider (~50 lines of Rust each).

**Input:** Tweet URL or Twitter handle
**Output:** Markdown tweet/thread digest with metrics and media links

#### 4.4.2 Xiaohongshu Extractor

**Parsing approach:** Third-party API adapter only.

**Why DIY is infeasible (even more than Twitter):** Xiaohongshu has four anti-scraping layers: (1) JS-signed API requests — every call requires cryptographic headers (`x-s`, `x-t`) computed by obfuscated JavaScript that changes approximately monthly with each app update; (2) TLS fingerprinting — requests from non-browser TLS stacks are rejected at the handshake level; (3) device registration — most endpoints require a hardware-fingerprinted device ID with attestation; (4) captcha walls — Cloudflare + custom captcha trigger after ~10 unregistered requests.

OpenCLI covers 166 services but has no Xiaohongshu CLI — confirming that even browser-based extraction is not viable. All working solutions are paid third-party APIs that maintain Android device farms and continuously reverse-engineer the signing algorithm.

**Third-party references:**

| Project | What We Borrowed | Rationale |
|---------|-----------------|-----------|
| **OpenCLI** | Absence confirms infeasibility | Xiaohongshu is the only major platform missing from OpenCLI's 166-service catalog. Every other major service (Twitter, YouTube, Reddit, GitHub, Bilibili, WeChat) has a CLI. This absence is strong evidence that Xiaohongshu's anti-scraping cannot be bypassed even with a browser. |
| **MinerU, Open Notebook** | Not applicable | Neither project handles social media APIs. |

**Implementation:** Same adapter pattern as Twitter, supporting Apify, Bright Data, Oxylabs, and ScrapeCreators. Xiaohongshu data is typically 2–3× more expensive than Twitter due to higher scraping difficulty.

**Input:** Xiaohongshu note URL
**Output:** Markdown with note content, images, and engagement metrics

#### 4.4.3 Podcast Extractor

**Parsing approach:** Two-step — RSS for metadata, Whisper API for transcription.

**Step 1 (metadata):** Parse the podcast RSS feed (reusing the same `feed-rs` logic as `RssExtractor`). Extract episode titles, descriptions, publication dates, and audio file URLs from `<enclosure>` tags.

**Step 2 (transcription):** Send the audio URL to OpenAI Whisper API (`POST /v1/audio/transcriptions`). Fallback option: local `whisper-rs` (Rust binding to whisper.cpp) for offline transcription. Output is merged with metadata into a single Markdown document with timestamped transcript sections.

**Third-party references:**

| Project | What We Borrowed | Rationale |
|---------|-----------------|-----------|
| **Open Notebook** | RSS + Whisper API pipeline | Open Notebook's `source.py` configures `audio_provider="openai"` and `audio_model="whisper-1"` in the `content_state` dict before calling `extract_content()`. Content-core then handles both the RSS parsing and the Whisper transcription. This validates the two-step approach. OpenCLI's `clis/apple-podcasts/episodes.js` uses the public iTunes Search API for podcast metadata (free, no auth) — an alternative metadata source if the RSS feed is unavailable. |

**Input:** Podcast RSS feed URL or direct audio URL
**Output:** Markdown with episode metadata and transcript

## 5. Third-Party Reference Matrix

| Cowiki Extractor | Open Notebook (content-core) | OpenCLI | MinerU |
|-----------------|------------------------------|---------|--------|
| **PDF** | ✅ Primary engine | — | ✅ Pipeline architecture reference |
| **DOCX** | ✅ Primary engine | — | ✅ OOXML parsing depth validates delegation |
| **PPTX** | ✅ Primary engine | — | ✅ Shape-type dispatch and reading-order reference |
| **Markdown** | — | — | — |
| **CSV** | — | — | — |
| **XLSX** | — | — | ✅ calamine approach validated |
| **URL** | ✅ Primary engine | — | — |
| **GitHub** | — | ✅ Read-only API access validated | — |
| **RSS** | — | — | — |
| **YouTube** | ✅ content-core wraps InnerTube | ✅ InnerTube API validated (no key needed) | — |
| **EPUB** | ✅ Primary engine | — | — |
| **Reddit** | — | ✅ `.json` endpoint + anonymous access validated | — |
| **OCR** | ✅ Primary engine | — | ✅ PaddleOCR pipeline validates delegation |
| **Twitter/X** | — | ✅ DIY infeasibility proven (browser required) | — |
| **小红书** | — | ✅ DIY infeasibility proven (no CLI exists) | — |
| **Podcast** | ✅ RSS + Whisper API pipeline | — | — |

## 6. Phased Delivery Plan

### Phase 1 — This Issue (#31)
- [ ] `crates/extractor/` crate with `SourceExtractor` trait + `ExtractorRegistry`
- [ ] 9 extractors: PDF, DOCX, PPTX, Markdown, CSV, XLSX, URL, GitHub, RSS
- [ ] Integration with `POST /api/ingest`
- [ ] `"auto"` type detection from filename extension or magic bytes
- [ ] `encoding: "base64"` support for binary file transfer
- [ ] Original file preservation + structured error handling (`extracted: false`)

### Phase 2 (#35, #36)
- [ ] YouTube, EPUB, Reddit, OCR extractors

### Phase 3 (#37, #38)
- [ ] Twitter/X, Xiaohongshu, Podcast extractors

## 7. Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Complex documents (PDF, DOCX, PPTX) | `content-core` subprocess | Avoids reimplementing thousands of lines of format-specific parsing; leverages mature Python libraries (Docling, PyMuPDF, mammoth) |
| Structured formats (CSV, XLSX, RSS) | Pure Rust crates | Simple enough for Rust-native; zero runtime overhead |
| Binary transfer | Base64 + `encoding` field | Backward-compatible JSON API; simpler than multipart file upload |
| Type detection | Explicit `source_type` + `"auto"` | Precision for MCP agent calls; convenience for human web UI use |
| Original files | Always preserved; never overwritten | User input safety; enables future re-extraction with improved extractors |
| Error handling | Save original, return `extracted: false` with error message | Input never lost; user gets actionable feedback |
| GitHub auth | Anonymous (60 req/hr) + optional token | Sufficient for public repos; no forced configuration |
| content-core dependency | Optional; graceful degradation | Best extraction quality when installed; pure Rust otherwise |
| URL extraction | content-core preferred, scraper fallback | Best results via Python engine; basic functionality without it |
