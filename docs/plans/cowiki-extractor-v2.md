# Cowiki-Extractor v2 重构方案

## 核心思路

不再从零实现每个格式的解析器，而是**在 third-party 已有成果之上改进**：

- **content-core**（Open Notebook 底层）已覆盖 PDF/DOCX/PPTX/YouTube/EPUB/OCR 等最复杂的格式
- **OpenCLI** 验证了 YouTube（InnerTube API）、Reddit（`.json` endpoint）、GitHub 等服务的提取方法
- **MinerU** 提供了 XLSX 表格解析和 OOXML 结构的深度参考

我们在 Rust 侧只做**轻量格式**（Text/Markdown/CSV/XLSX/RSS）和 **API 调用**（GitHub/Reddit/Phase 3）。复杂文档格式通过 `universal.rs` 调用 content-core。

## 架构

```
crates/extractor/src/
├── lib.rs              # trait + create_default_registry()
├── error.rs            # ExtractError (不变)
├── types.rs            # SourceType(13) + AuthStrategy (新增 Universal)
├── registry.rs         # ExtractorRegistry (不变)
│
├── universal.rs        # 🆕 调用 content-core subprocess
│   SourceType::Pdf, Docx, Pptx, Epub, YouTube, Ocr
│
├── text.rs             # ✅ 保留
│   SourceType::Text
├── markdown.rs         # ✅ 保留
│   SourceType::Markdown
├── csv.rs              # ✅ 保留
│   SourceType::Csv
├── xlsx.rs             # ✅ 保留
│   SourceType::Xlsx
├── github.rs           # ✅ 保留
│   SourceType::GitHubRepo, GitHubIssue
├── rss.rs              # ✅ 保留
│   SourceType::Rss
│
├── url.rs              # 🔄 重构：优先 content-core，回退 scraper
│   SourceType::Url
│
└── (Phase 3 新增)
    twitter.rs, xiaohongshu.rs, podcast.rs
```

## 删除的文件

| 文件 | 原因 |
|------|------|
| `pdf.rs` | content-core 替代 |
| `docx.rs` | content-core 替代 |
| `pptx.rs` | content-core 替代 |
| `ooxml_images.rs` | content-core 内部处理图片 |

## 新增的 SourceType

```rust
pub enum SourceType {
    // ... 现有 ...
    Universal,  // 🆕 通用类型 → content-core 自动识别
}
```

用户可以用 `"universal"` 传任意文件，content-core 自动判断格式。

## 逐个 Extractor 方案

### UniversalExtractor（content-core 封装）

**覆盖的 Type**：Pdf, Docx, Pptx, Epub, YouTube, Ocr, Universal

**参考**：Open Notebook `source.py` 第 78 行 —— `await extract_content(content_state)`

**实现**：

```rust
// universal.rs
pub struct UniversalExtractor;

impl SourceExtractor for UniversalExtractor {
    async fn extract(&self, input: ExtractInput) -> Result<ExtractResult, ExtractError> {
        // 1. 将 base64 内容解码为临时文件
        let temp_dir = TempDir::new()?;
        let ext = detect_extension(&input);
        let temp_path = temp_dir.path().join(format!("input.{}", ext));
        std::fs::write(&temp_path, &decode_content(&input)?)?;

        // 2. 调用 content-core
        let output = std::process::Command::new(PYTHON_BIN)
            .arg("-c")
            .arg(format!(
                "import asyncio,content_core; \
                 result = asyncio.run(content_core.extract_content(file_path='{temp_path}')); \
                 print(result.content)"
            ))
            .output()?;

        let text = String::from_utf8_lossy(&output.stdout).to_string();
        // 3. 返回 ExtractResult
    }
}
```

**依赖**：服务器需安装 Python 3.10+ 和 `pip install content-core`

**优雅降级**：调用前检查 content-core 是否可用。不可用时返回带安装指引的错误。

### TextExtractor

**参考**：无（太简单）  
**实现**：透传 + 空白规范化。  
**状态**：已完成 ✅

### MarkdownExtractor

**参考**：无  
**实现**：验证 frontmatter + 规范化缩进。  
**状态**：已完成 ✅

### CsvExtractor

**参考**：无  
**实现**：`csv` crate → 自动检测分隔符 → Markdown GFM table。  
**状态**：已完成 ✅

### XlsxExtractor

**参考**：MinerU `xlsx_converter.py` —— calamine 同款方案  
**实现**：`calamine` crate → 逐 sheet → Markdown table。  
**状态**：已完成 ✅

### UrlExtractor（重构）

**参考**：
- Open Notebook `source.py`：content-core 提取网页正文
- OpenCLI：不需要，URL 提取不是 OpenCLI 的核心功能
- MinerU：不涉及

**实现**：
```
1. 尝试 content-core 提取（效果最好，处理 JS 渲染、反爬）
2. 失败 → 回退到 scraperscraper + html2md（纯 Rust，无额外依赖）
3. 图片下载：content-core 内部已处理；回退方案手动下载 base64 嵌入
```

### GitHubExtractor

**参考**：OpenCLI `clis/github/auth.js` —— 浏览器 Cookie 登录；`clis/reddit/` —— `.json` endpoint 无需 auth

**实现**：`octocrab` crate → repo README + 文件树；issue 标题+正文+评论。  
**匿名模式**：60 req/hr（公开仓库足够）。  
**Token 模式**：可选，存 DB user settings。  
**状态**：已完成 ✅

### RssExtractor

**参考**：无  
**实现**：`feed-rs` crate → 解析 RSS/Atom/JSON Feed → Markdown 文章列表。  
**状态**：已完成 ✅

#### Phase 2

### YouTubeExtractor

**参考**：
- **OpenCLI** `clis/youtube/transcript.js`（656 行）：`page.goto()` → `page.evaluate()` → `ytInitialPlayerResponse` → captions → `fetch()` 字幕 XML → 解析 srv3/json3 → `[{start, dur, text}]`。**重点：不需要 API key！** InnerTube API key 从页面 JS 提取，字幕 URL 直接可用。
- **Open Notebook**：content-core → youtube-transcript-api

**两个实现方案**：
| | 方案 A：content-core | 方案 B：Rust 原生 |
|---|---|---|
| **复杂度** | 一行 Python | ~200 行 Rust |
| **依赖** | Python + content-core | `reqwest` + `quick-xml` |
| **稳定性** | content-core 维护 | 自行维护 |

**选择**：走 content-core。content-core 封装了 OpenCLI 同样的逻辑（InnerTube API + srv3 字幕 XML），无需 Rust 侧重写。

### EpubExtractor

**方案**：content-core subprocess（或 Rust `epub` crate）。content-core 已支持 EPUB。

### RedditExtractor

**参考**：OpenCLI `clis/reddit/read.js` —— append `.json` 到任何 Reddit URL 就返回 JSON。**最友好的大平台 API**。

**实现**：`reqwest::get("https://reddit.com/r/rust.json")` → 解析 JSON → Markdown。匿名 60 req/min，可选 OAuth2 提额。

**状态**：Rust 侧新加，约 50 行。

### OcrExtractor（图片→文字）

**参考**：
- **MinerU** `mineru/model/ocr/`：PaddleOCR（DBNet + CRNN + 段落合并 + 阅读顺序），需要 PyTorch
- **Open Notebook**：content-core → OCR 引擎

**方案**：content-core subprocess。OCR 是整个系统里最复杂的任务（深度学习模型），在 Rust 侧重写不现实。

#### Phase 3

### TwitterExtractor

**参考**：**OpenCLI** `clis/twitter/timeline.js`（212 行）：`Strategy.COOKIE` → 浏览器 cookie (`ct0` + `auth_token`) + 硬编码公开 `BEARER_TOKEN`（Twitter 网页客户端自带的 `AAAA...`）+ 内部 GraphQL API（`/i/api/graphql/{queryId}/{Endpoint}`）

**OpenCLI 验证了什么**：
1. `BEARER_TOKEN` 是**公开可用的**（Twitter 网页 JS bundle 里的常量）
2. `ct0` CSRF token 从浏览器 cookie 读（服务端无法获取）
3. 所有请求在**浏览器内部发**（`page.evaluate(fetch(...))`）→ 自带 cookie + TLS 指纹
4. **这就是为什么服务端 Rust 不能 DIY** —— 缺了浏览器 cookie + TLS 指纹

**方案**：第三方 API 适配器（ScrapeCreators / Apify / SocialData）。Rust 侧只做 HTTP 转发。

### 小红书 Extractor

**参考**：**OpenCLI 没有小红书 CLI**（唯一缺失的主流平台），验证了 DIY 不可行

OpenCLI 覆盖了 166 个服务，不存在小红书实现。原因：四层反爬 ——
1. JS 签名请求（每月变化）
2. TLS 指纹检测  
3. 设备注册
4. Captcha 墙

**方案**：第三方 API 适配器，同 Twitter。

### PodcastExtractor

**参考**：Open Notebook —— RSS 解析元数据 + Whisper API 转录

**实现**：
1. 元数据：已有 `RssExtractor` 解析 podcast RSS feed
2. 转录：OpenAI Whisper API（已有 key）
3. 输出：`## 转录\n\nSpeaker: Text...`

## 第三方参考关系表

| cowiki Extractor | Open Notebook | OpenCLI | MinerU |
|-----------------|---------------|---------|--------|
| **Universal**（PDF/DOCX/PPTX/EPUB/OCR/YouTube） | ✅ content-core 引擎 | — | ✅ OOXML 解析参考 |
| **Text** | — | — | — |
| **Markdown** | — | — | — |
| **CSV** | — | — | — |
| **XLSX** | — | — | ✅ calamine 方案验证 |
| **URL** | ✅ content-core（优先） | — | — |
| **GitHub** | — | ✅ 验证了需要读权限 | — |
| **RSS** | — | — | — |
| **Reddit** | — | ✅ `.json` endpoint 验证 | — |
| **Twitter** | — | ✅ 验证了 BEARER_TOKEN 公开 + DIY 不可行 | — |
| **小红书** | — | ✅ 验证了无实现（全平台缺失） | — |
| **Podcast** | ✅ RSS + Whisper API | — | — |

## 删除 vs 保留 vs 新增

| 代码 | 决定 | 原因 |
|------|------|------|
| `pdf.rs` (134行) | ❌ 删除 | content-core 替代 |
| `docx.rs` (171行) | ❌ 删除 | content-core 替代 |
| `pptx.rs` (142行) | ❌ 删除 | content-core 替代 |
| `ooxml_images.rs` (127行) | ❌ 删除 | content-core 处理 |
| `url.rs` (271行) | 🔄 重构 | 简化：优先 content-core |
| `text.rs` (46行) | ✅ 保留 | |
| `markdown.rs` (78行) | ✅ 保留 | |
| `csv.rs` (72行) | ✅ 保留 | |
| `xlsx.rs` (90行) | ✅ 保留 | |
| `github.rs` (263行) | ✅ 保留 | |
| `rss.rs` (140行) | ✅ 保留 | |
| `universal.rs` | 🆕 新增 | content-core subprocess 封装 |
| `error.rs` | ✅ 保留 | |
| `types.rs` | 🔄 更新 | 新增 Universal type |
| `registry.rs` | ✅ 保留 | |
| `lib.rs` | 🔄 更新 | 注册表变更 |

**净变化**：删除 ~570 行 Rust，新增 ~100 行（universal.rs），换取 PDF/DOCX/PPTX 的完整解析能力（含图片、公式、表格、OCR）。
