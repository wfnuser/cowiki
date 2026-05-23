# CoWiki 产品深度思考 — 2026-05-23

## 一、当前功能状态

### 已完成 ✅
- GitHub OAuth 登录
- 用户注册 + API key 颁发
- Personal Space（自动创建 + Getting Started 欢迎页）
- Team Space（General 默认创建 + Team Space Home）
- Workspace CRUD（创建、列表、加入、重命名）
- 权限模型（owner / writer / reader）
- 每个 workspace 独立 Git repo
- 页面 CRUD（创建页面、创建文件夹、查看页面）
- 树状页面列表（文件夹展开/折叠）
- Source Ingest（URL / 文本）
- LLM 编译（source → wiki pages，增量编译）
- Submit（Personal 直接 commit，Team 走 review）
- Review 审核流程（approve / reject + diff 查看）
- 语义搜索（pgvector）
- 去重检测（提交时 embedding 比对）
- Notion 风格 sidebar（不跳转，展开式）
- Breadcrumb 导航 + 操作按钮
- Discover 公开空间发现页

### 功能存在但有问题 ⚠️
- **页面编辑**：无法在浏览器内编辑，只能查看。这是最大的功能缺口。
- **Review UI**：基础 diff 展示，但不是 GitHub PR 级别的体验。没有评论功能。
- **搜索**：只在顶部 breadcrumb 区域（workspace 内的搜索），没有全局搜索入口。
- **Compile**：调用 OpenAI 但不区分 workspace repo（还在用旧的全局 repo 做 compile）。
- **URL 路由**：navigate 了但刷新页面会丢失状态（没有从 URL 恢复页面选中状态）。
- **Ingest**：写到 userBranch 但没有指定 workspace repo，可能写到错误的 repo。

### 完全没做 ❌
- MCP Server（agent 接入）
- 浏览器内 Markdown 编辑器（Milkdown）
- Wikilink 自动链接和解析
- 知识图谱可视化
- 多维表格
- CRDT 实时协作编辑
- Desktop 应用（Tauri）
- Docker 一键部署
- 通知系统（有人提交了 review 等）
- 成员邀请 UI（后端有，前端没有入口）
- Workspace 设置页面（schema 配置、成员管理）
- 导出功能（Markdown / PDF / llms.txt）
- 版本历史查看（Git log 可视化）
- 移动端适配

---

## 二、核心架构问题

### 1. Compile 和 Ingest 没有走 workspace-scoped repo
现在 compile route 用的还是 `state.wiki_repo`（全局默认 repo），不是 `state.repo_manager.get(ws_slug)`。需要像 pages 那样加 workspace-scoped 的 compile 和 ingest 路由。

### 2. Git 操作的并发安全
虽然加了 per-branch write lock，但 `merge_to_main` 在持有 main 的锁时调用了 `write_file`，后者也会尝试获取锁——这可能导致死锁。需要审查锁的粒度。

### 3. 前端状态管理
MainLayout.tsx 已经 600+ 行，所有状态都在一个组件里。随着功能增加会变得不可维护。需要：
- 用 React Context 或 Zustand 做全局状态管理
- 拆分成更小的组件

### 4. 没有前端编辑能力
wiki 产品的核心是"编辑"。现在只能看不能改，这不是 wiki，是文档查看器。集成 Milkdown 或 Tiptap 是最高优先级。

### 5. 没有错误恢复
用户操作失败（网络断了、API 报错）时，UI 只显示一行红色文字。没有重试机制，没有 optimistic update。

---

## 三、产品经理视角：用户旅程断点

### 新用户第一次使用
1. ✅ GitHub 登录
2. ✅ 看到 Personal Space + General team space
3. ❌ 不知道该做什么——Home 页面太空了，没有引导
4. ❌ 点击 Getting Started 只能看不能编辑
5. ❌ 没有 onboarding 教程或交互引导

### 日常知识整理
1. ✅ 可以 ingest source（文本/URL）
2. ✅ 可以 compile 生成 wiki 页面
3. ❌ 无法编辑生成的页面（只能重新 compile）
4. ❌ 没有搜索快捷键（Cmd+K）
5. ❌ 页面之间没有链接（wikilink）

### 团队协作
1. ✅ 可以创建 Team Space
2. ❌ 没有邀请成员的 UI（后端有，前端没有）
3. ❌ 没有通知（有人提了 review 不知道）
4. ❌ Review UI 太简陋

---

## 四、CTO 视角：技术债和风险

### 高风险
- **单点故障**：一个 Rust 进程 + 一个 PG 实例。进程挂了所有用户受影响。
- **Git 性能**：当 repo 变大（几千个文件），`list_files_recursive` 会变慢。需要缓存层。
- **OpenAI 依赖**：compile 和 search 完全依赖 OpenAI API。API 挂了 = 产品核心功能不可用。

### 中风险
- **数据丢失**：Git repo 在本地磁盘，没有备份策略。Docker volume 丢了 = 数据全没。
- **安全**：API key 明文存 PostgreSQL，没有 hash。GitHub OAuth secret 在 .env 里。
- **前端性能**：sidebar 加载所有 workspace 的页面列表，workspace 多了会卡。

### 低风险（但要注意）
- **DB 迁移**：用的是 raw SQL，没有迁移框架（sqlx-cli）。多次 `ALTER TABLE IF NOT EXISTS` 能跑但不优雅。
- **测试覆盖**：后端 0 个测试。前端 0 个测试。
- **代码组织**：`MainLayout.tsx` 600+ 行需要拆分。

---

## 五、架构师视角：如何走向生产

### Phase 1：MVP 可用（展会前 1-2 周）
1. **集成 Markdown 编辑器**（Milkdown/Tiptap）— 最重要
2. **Compile/Ingest 走 workspace repo** — 修复数据隔离 bug
3. **URL 状态恢复** — 刷新不丢页面
4. **Docker 一键部署** — Dockerfile + docker-compose 把所有东西打包
5. **成员邀请 UI** — 前端加入口

### Phase 2：产品打磨（展会后 1-2 月）
1. MCP Server（agent 接入）
2. Review UI 改版（GitHub PR 风格）
3. Wikilink 解析和双向链接
4. 全局搜索（Cmd+K）
5. 通知系统
6. Workspace 设置页面

### Phase 3：规模化（3-6 月）
1. Git 操作缓存层（Redis/DB 缓存页面列表）
2. 多 LLM provider 支持（Anthropic, Ollama, local models）
3. Desktop 应用（Tauri）
4. CRDT 实时编辑
5. 导出和 API

---

## 六、CEO 视角：商业和战略

### 竞争格局
- **Notion/飞书/Confluence** — 成熟的协作工具，但 AI 是后加的
- **llm-wiki-compiler 等开源工具** — 单人使用，没有协作
- **Mem0/Zep 等 memory 产品** — 面向开发者，不面向人
- **Augment Cosmos** — 最接近的竞品，但只做开发场景

### CoWiki 的差异化
1. **LLM 编译 + 多人协作 + 版本控制** — 三者结合的唯一产品
2. **开源 self-host** — 企业数据不出去
3. **Agent-native（MCP）** — 人和 AI 共同维护知识
4. **Git 做底层** — 工程师天然信任

### 商业模式路径
1. **开源社区** → GitHub stars, community adoption
2. **企业 self-host** → 付费支持和 SLA
3. **Cloud hosted** → SaaS（后期）
4. **Open core** → Pro 功能（SSO, RBAC, 审计日志, 高级分析）

### 关键指标
- GitHub stars（社区影响力）
- 活跃 workspace 数（产品粘性）
- 编译页面数（核心功能使用）
- Review 通过率（协作活跃度）
- Agent API 调用量（AI 集成深度）

### 最大风险
1. **没有编辑能力就不是 wiki** — 用户试用后发现不能编辑，直接流失
2. **LLM 编译质量不稳定** — 生成的 wiki 页面质量差，用户不信任
3. **冷启动问题** — 空的 wiki 没有价值，需要有好的 onboarding 引导
4. **vs Notion 的 "够用了"** — 很多团队用 Notion + AI sidebar 就能满足需求

### 战略建议
1. **先打开发者社区** — 开源 + 技术博客 + HN 发帖，让开发者用起来
2. **MCP 是杀手锏** — 让 Claude Code / Cursor 的用户能把 agent 经验自动沉淀到 wiki，这是 Notion 做不到的
3. **展会故事** — "我们团队过去一周用 CoWiki，agent 自动沉淀了 47 条知识，新同事第一天就有老员工的经验"
4. **不要做 Notion 杀手** — 定位为"AI agent 的知识共享基础设施"，不是"又一个文档工具"

---

## 七、明天验收清单

1. 登录 → 看到 Personal Space + General Team Space
2. Personal Space 有 Getting Started 页面
3. Team Space 有 Team Space Home 页面
4. 点 + 可以创建页面和文件夹
5. Add Source 可以粘贴文本
6. Compile 可以生成 wiki 页面
7. Submit 可以提交（Personal 直接 commit）
8. Discover 可以看到公开空间
9. 所有按钮都有实际功能（没有假按钮）
