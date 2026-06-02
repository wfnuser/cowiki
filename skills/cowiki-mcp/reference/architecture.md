# cowiki MCP Server — 架构参考

> 此文件从项目文档中提取，供 cowiki-mcp skill 自包含使用。

## 架构

MCP 服务是独立进程，作为 MCP→REST 代理：

```
MCP Client ──→ cowiki-mcp (:8080) ──→ cowiki-server REST API (:3000) ──→ core/db
              (rmcp + hyper)              (axum)
```

两个进程共享同一套认证（Bearer token 透传）。

## 协议

- **Transport**: Streamable HTTP（MCP 协议 2025-03-26）
- **SDK**: rmcp v1.7
- **端点**: `http://localhost:8080/mcp`
- **认证**: `Authorization: Bearer <api_key>`（所有请求均需）

## 启动服务

```bash
# 终端 1: 启动 REST API
cargo run -p cowiki-server

# 终端 2: 启动 MCP 代理
cd cowiki-mcp-server && cargo run
```

MCP server 通过 `COWIKI_BASE_URL` 环境变量连接到后端（默认 `http://localhost:3000`）。默认端口 `8080`。使用独立 `.env` 配置（见 `cowiki-mcp-server/.env.example`）。

## 获取 API Key

```bash
curl -X POST http://localhost:3000/api/auth/register \
  -H "Content-Type: application/json" \
  -d '{"name":"my-agent","email":"agent@example.com"}'
# → {"api_key": "cw_xxxxxxxxxx", ...}
```

## 工具列表（10 个）

所有工具实现在 `cowiki-mcp-server/src/server.rs`，调用后端 `crates/server/src/routes/` 对应端点。

### 知识操作

| 工具 | 方法 | 后端 API | 说明 |
|------|------|----------|------|
| `cowiki_ingest` | POST | `/api/ingest` | 摄入源文档到个人空间 |
| `cowiki_compile` | POST | `/api/compile` | 编译个人空间源文档为 wiki 页面 |
| `cowiki_read` | GET | `/api/pages/{slug}` | 读取 main 空间页面 |
| `cowiki_write` | POST | `/api/pages` | 创建/编辑个人空间页面 |
| `cowiki_list` | GET | `/api/pages` | 列出 main 空间所有页面 |
| `cowiki_search` | GET | `/api/search` | 语义搜索 main 空间 |
| `cowiki_submit` | POST | `/api/submit` | 提交个人页面到审核队列 |

写入操作自动使用当前用户个人空间 (`user/{user.id}`)，读取/搜索默认使用 `main` 分支。

### 审核操作

| 工具 | 方法 | 后端 API | 说明 |
|------|------|----------|------|
| `cowiki_review_list` | GET | `/api/reviews` | 待审核列表 |
| `cowiki_review_get` | GET | `/api/reviews/{id}` | 审核详情 + diff |
| `cowiki_review_decide` | POST | `/api/reviews/{id}` | 批准/拒绝 |

### 工作区操作（18 tools）

| 工具 | 方法 | 后端 API | 说明 |
|------|------|----------|------|
| `cowiki_workspace_list` | GET | `/api/workspaces` | 列出我的工作区 |
| `cowiki_workspace_create` | POST | `/api/workspaces` | 创建工作区 |
| `cowiki_workspace_join` | POST | `/api/workspaces/{slug}/join` | 加入公开工作区 |
| `cowiki_workspace_rename` | POST | `/api/workspaces/{slug}/rename` | 重命名 |
| `cowiki_workspace_delete` | DELETE | `/api/workspaces/{slug}` | 删除 |
| `cowiki_workspace_invite` | POST | `/api/workspaces/{slug}/invite` | 邀请成员 |
| `cowiki_workspace_members` | GET | `/api/workspaces/{slug}/members` | 成员列表 |
| `cowiki_workspace_remove_member` | POST | `.../members/remove` | 移除成员 |
| `cowiki_workspace_change_role` | POST | `.../members/role` | 修改角色 |
| `cowiki_invitation_list` | GET | `/api/invitations/pending` | 待处理邀请 |
| `cowiki_invitation_accept` | POST | `/api/invitations/{id}/accept` | 接受邀请 |
| `cowiki_invitation_reject` | POST | `/api/invitations/{id}/reject` | 拒绝邀请 |
| `cowiki_workspace_pages` | GET | `/api/workspaces/{slug}/pages` | 工作区页面列表 |
| `cowiki_workspace_read` | GET | `/api/workspaces/{slug}/pages/{slug}` | 读取工作区页面 |
| `cowiki_workspace_write` | POST | `/api/workspaces/{slug}/pages` | 创建/编辑工作区页面 |
| `cowiki_workspace_ingest` | POST | `/api/workspaces/{slug}/ingest` | 摄入文档到工作区 |
| `cowiki_workspace_compile` | POST | `/api/workspaces/{slug}/compile` | 编译工作区文档 |

## 典型工作流

```
个人空间:  ingest → compile → read → write → submit
知识查询:  search → read → list
参与审核:  review_list → review_get → review_decide
工作区协作: workspace_list → workspace_create → workspace_invite → workspace_pages → workspace_write
```

## 错误码

| Code | 含义 | 常见原因 |
|:----:|------|------|
| `-32700` | JSON 解析错误 | 请求体格式错误 |
| `-32602` | 无效参数 | 工具参数类型/缺失 |
| `-32603` | 服务器内部错误 | 后端 API 异常、网络不通 |
| `-32001` | 未授权 | 缺少或无效 Authorization |
| `-32002` | 资源未找到 | slug 不存在 |
| `-32003` | 禁止访问 | 无权限 |

## 配置示例

### VS Code (`.vscode/mcp.json`)

```json
{
  "servers": {
    "cowiki-mcp": {
      "url": "http://localhost:8080/",
      "type": "http",
      "headers": {
        "Authorization": "Bearer cw_your_api_key_here"
      }
    }
  }
}
```

### Claude Desktop / Claude Code (`~/.claude.json`)

```json
{
  "mcpServers": {
    "cowiki-mcp": {
      "type": "http",
      "url": "http://localhost:8080/",
      "headers": {
        "Authorization": "Bearer cw_your_api_key_here"
      }
    }
  }
}
```
