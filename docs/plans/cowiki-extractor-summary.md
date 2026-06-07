# Cowiki-Extractor 摘要

## 工作流

```
用户上传 (base64/URL/文本)
  │
  ├─ source_type="pdf"  ──▶ PdfExtractor   ──▶ sources/report.md
  ├─ source_type="auto" ──▶ 自动检测类型    ──▶ sources/data.md
  ├─ source_type="url"  ──▶ UrlExtractor   ──▶ sources/article.md
  └─ source_type="text" ──▶ TextExtractor  ──▶ sources/note.md

同时保存: sources/report.pdf (原始文件，永不丢失)
```

提取成功 → `{ extracted: true, filename: "report.md" }`
提取失败 → `{ extracted: false, extract_error: "原因" }` — 原始文件仍然保存

## ExtractorRegistry

```rust
pub trait SourceExtractor: Send + Sync {
    fn supported_types(&self) -> Vec<SourceType>;
    async fn extract(&self, input: ExtractInput) -> Result<ExtractResult, ExtractError>;
}

// 按 source_type 自动分发
pub struct ExtractorRegistry {
    extractors: HashMap<SourceType, Arc<dyn SourceExtractor>>,
}
```

## Phase 1 Extractor 速览

### TextExtractor
- **无依赖**，纯透传 + 空白规范化
- 最简单，验证整条链路用

### UrlExtractor（增强版）
- **Crate:** `reqwest` + `html2md`
- 抓取网页 → 提取正文 → 转 Markdown
- **关键改进:** 当前返回 raw HTML，这个返回干净的 LLM 可读 Markdown

### MarkdownExtractor
- **无依赖**，验证 frontmatter + 规范化缩进

### CsvExtractor
- **Crate:** `csv`
- 自动检测分隔符 → 首行作表头 → 输出 GFM table

### PdfExtractor
- **Crate:** `pdf-extract`（纯 Rust）
- 逐页提取文本，页间 `---` 分隔
- **不做 OCR**（Phase 2）

### DocxExtractor
- **Crate:** `docx-rs`
- 解析 OOXML：标题层级 → `#`、列表、表格、粗体/斜体
- **简化版**（不做公式、图片、文本框），参考 MinerU 但不追求完整度

### PptxExtractor
- **Crate:** `pptx-rs` 或 `undoc`
- 每页幻灯片 → `## Slide N`
- 提取文本框 + 表格 + 演讲者备注

### XlsxExtractor
- **Crate:** `calamine`（纯 Rust）
- 每个 Sheet → 一个 Markdown section + table

### GitHubExtractor
- **Crate:** `octocrab`
- Repo → README + 目录树 → Markdown
- Issue/PR → 标题 + 正文 + 评论
- **Token:** 用户 Settings 里配，没配走匿名（60req/hr）

### RssExtractor
- **Crate:** `feed-rs`
- RSS/Atom/JSON Feed → 元信息 + 文章列表 Markdown

## API 改动

```json
// 新增字段（均可选，向后兼容）
{
  "source_type": "pdf",       // 新增 10+ 种，老值 "url"/"text" 不变
  "encoding": "base64",       // 仅二进制文件需要
  "filename": "report.pdf"    // auto 检测用
}
```

## 关键设计决策

| 决策 | 结论 |
|------|------|
| 二进制传输 | base64 + `encoding` 字段 |
| 类型确定 | 显式指定 + `"auto"` (扩展名/magic bytes) |
| 原始文件 | 始终保留，绝不覆盖 |
| 错误处理 | 保留原始 + 返回错误原因 |
| 实现顺序 | 先骨架 + TextExtractor 跑通 → 逐个加 |

## 三个参考项目

| 项目 | 核心策略 | 对 cowiki 的启发 |
|------|---------|-----------------|
| **OpenCLI** | 浏览器 CDP → `page.evaluate(fetch(...))` → 自带 cookie/TLS/UA | Registry 模式、Strategy 枚举、YouTube InnerTube、Reddit `.json` |
| **MinerU** | 格式特定 Converter → Middle JSON → 统一 Markdown | 多格式 pipeline、DOCX/PPTX/XLSX 解析、OCR 预处理流程 |
| **Open Notebook** | `content-core.extract_content()` 统一入口 + Esperanto 多 AI 提供商 | Podcast 的 Whisper API 转录方案、credential 管理模式 |

## Phase 2 Extractor 速览

### YouTubeExtractor
- **Crate:** `reqwest` + `quick-xml`
- **Auth:** 不需要！API key 从 watch 页面 `ytInitialPlayerResponse` 提取
- 流程：fetch 页面 HTML → 解析 JSON → 找 caption tracks → 下载 srv3 XML → 解析 `[{start, dur, text}]` → Markdown
- **参考:** OpenCLI `clis/youtube/transcript.js`（InnerTube API + 浏览器内 fetch 拦截 → 我们改用纯 `reqwest` 同样的 URL）
- **局限:** 部分视频无字幕；~100 req/天/IP

### EpubExtractor
- **Crate:** `epub`（纯 Rust，ZIP + XHTML）
- 解压 EPUB → spine 顺序读章节 XHTML → 去标签转 Markdown → TOC
- **局限:** DRM 加密不可读

### RedditExtractor
- **Crate:** `reqwest` — **最简单的大平台 API**：URL 后面加 `.json` 即返回 JSON
- **Auth:** 匿名即可（60 req/min），可选 OAuth 提额
- `r/rust/hot.json` → 热帖；`/comments/{id}.json` → 帖+评论；`/user/{name}/submitted.json` → 用户帖
- **参考:** OpenCLI `clis/reddit/`（验证了 `.json` endpoint 的可行性）

### OcrExtractor（图片→文字）
- **Crate:** `leptess`（tesseract-sys）+ `image`
- **系统依赖:** `libtesseract`
- 流程：解码图片 → 灰度/二值化 → Tesseract OCR → 段落合并
- **参考:** MinerU `mineru/model/ocr/`（PaddleOCR 流程：检测→识别→后处理，我们简化为 Tesseract 单步）
- **局限:** 手写识别差；无排版分析

## Phase 3 Extractor 速览

**核心原则：绝不 DIY scraper，只对接第三方 API。**

### TwitterExtractor
- **为什么不能 DIY：** OpenCLI 的方案依赖浏览器 CDP + cookie session，服务器端 Rust 无此条件。硬编码 `BEARER_TOKEN`（`AAAA...`）是公开的，但需要配合 `ct0` + `auth_token` cookie，且从非浏览器环境发请求会被 TLS 指纹封杀
- **方案：** 用户选第三方 API 服务商（ScrapeCreatures/Apify/SocialData）→ 配 API key → Extractor 作为轻量适配层转发请求
- **局限:** 必须付费；隐私账号不可达

### XiaohongshuExtractor（小红书）
- **为什么比 Twitter 更困难：** 四层防护 — ① API 签名（`x-s`/`x-t` header，JavaScript 混淆，每月更新）；② TLS 指纹（非浏览器 TLS 栈被直接拒绝）；③ 设备注册（硬件指纹 + 设备证明）；④ 验证码墙（~10 次未注册请求触发）
- **OpenCLI 也没有小红书 CLI**（166 个服务中唯一缺失的主流平台），验证了 DIY 的不可行性
- **方案：** 同 Twitter — 轻量 adapter 对接第三方 API（Apify/ScrapeCreators/Bright Data/Oxylabs）
- **成本：** 小红书数据通常是 Twitter 的 2-3 倍价格（需维护 Android 设备农场 + 更频繁的签名更新）
- **局限:** 必须付费；私有笔记不可达；可能有水印

### PodcastExtractor（音频→文字）
- **Crate:** `reqwest`（+ 可选 `whisper-rs`）
- **两步流程：** ① RSS feed 元数据解析（同 Phase 1 RssExtractor）；② 音频 → Whisper API 转录
- **转录方案：** 默认 OpenAI Whisper API（已有 key，~$0.006/min）；可选 `whisper-rs`（免费但需要模型 + C++ 构建）
- **参考:** Open Notebook — `content-core` 统一入口 + Esperanto 的 `audio_provider`/`audio_model` 配置 → Whisper API
- **局限:** 1h 播客 ≈ $0.36；无说话人分离；非英语准确度不一

## Auth 策略总览

```
NoAuth    → PDF, CSV, Markdown, RSS, YouTube, EPUB, Reddit, OCR
ApiKey    → GitHub, Reddit(OAuth提额), Twitter/X, 小红书, Podcast(Whisper)
Cookie    → (未来)
```

## 三个参考项目对各 extractor 的贡献

| Extractor | OpenCLI | MinerU | Open Notebook |
|-----------|---------|--------|---------------|
| YouTube | ✅ 主参考（InnerTube API） | — | ✅ 验证方案（content-core） |
| Reddit | ✅ 主参考（`.json` endpoint） | — | — |
| OCR | — | ✅ 主参考（预处理流程） | — |
| Twitter | ✅ 验证了 DIY 不可行 | — | — |
| 小红书 | ✅ 验证了无实现（全平台缺失） | — | — |
| Podcast | ✅ 元数据（iTunes API） | — | ✅ 转录（Whisper API） |
