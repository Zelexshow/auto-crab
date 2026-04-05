# Auto-Crab 知识库集成设计

> 本文档描述 auto-crab 作为**生产端**在共享知识库体系中的角色与实现。
> 完整的知识库体系规范参见共享设计文档：`../auto-notebook/KNOWLEDGE-BASE-DESIGN.md`

---

## 一、定位

```
auto-crab (生产端)              auto-notebook (消费端)
┌──────────────────┐           ┌──────────────────────┐
│ 定时任务           │           │ 文件监控 (watchdog)    │
│ AI Agent 生成文档   │──写入──→  │ 自动检测变更           │
│ 结构化 frontmatter │  vault   │ 解析 + 分块 + 嵌入     │
│ 路由到分类目录      │           │ 向量存储 + 元数据存储   │
└──────────────────┘           │ RAG 检索 + 问答        │
                               │ 定期复盘报告           │
                               └──────────────────────┘
```

auto-crab 负责：
- 通过定时任务调用 LLM 生成结构化文档（日报、分析报告等）
- 按任务名称关键词自动路由到分类目录
- 生成符合规范的 YAML frontmatter
- 写入共享 vault 路径

auto-notebook 负责：
- 监控 vault 变更，自动解析、分块、嵌入
- 提供 RAG 检索和问答接口
- 生成定期复盘报告（写回 `reviews/`）

---

## 二、共享 Vault 路径

两个系统指向同一个知识库目录：

**auto-crab** (`auto-crab.toml`):
```toml
[knowledge]
enabled = true
vault_path = "C:/Workspace/AI-Assistant/self-rag"
```

**auto-notebook** (`config.yaml`):
```yaml
sources:
  - path: C:/Workspace/AI-Assistant/self-rag
    type: obsidian
    watch: true
```

---

## 三、目录结构

```
{vault_path}/
├── invest-explore/          # 投资研究
│   └── {YYYY-MM-DD}/
│       └── {HHMM}-{标题}.md
├── boss-explore/            # 创业探索
├── hot-news/                # 资讯热点
├── tech-notes/              # 技术笔记
├── thinking/                # 思考记录
├── reference/               # 长期参考资料
├── general/                 # 未分类内容（兜底）
└── reviews/                 # auto-notebook 生成的复盘报告
```

---

## 四、路由规则

`resolve_vault_subdir()` 根据任务名称中的关键词匹配分类目录：

| 分类 | 配置键 | 目录 | 触发关键词 |
|------|--------|------|-----------|
| 投资 | `invest` | `invest-explore` | 投资、行情、盘、invest、理财、简报 |
| 创业 | `boss` | `boss-explore` | 创业、boss、startup、副业 |
| 资讯 | `news` | `hot-news` | 日报、新闻、选题、news、热点 |
| 技术 | `tech` | `tech-notes` | 技术、编程、代码、tech、coding、架构、科技 |
| 思考 | `thinking` | `thinking` | 思考、复盘、总结、周记、反思、thinking |
| 参考 | `reference` | `reference` | 参考、指南、文档、reference、guide |
| 兜底 | `default` | `general` | 以上均不匹配时 |

路由映射可在配置文件 `[knowledge].routing` 中自定义。

---

## 五、Frontmatter 规范

auto-crab 生成的每个 `.md` 文件包含以下 YAML frontmatter：

```yaml
---
task: 科技日报
date: 2026-04-04
time: "08:02"
category: hot-news
tags: [科技, 热点, proj:auto-crab]
source: auto-crab
summary: "一句话摘要，提升 RAG 检索命中率"
related: []
---
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `task` | string | 任务名称，对应定时任务的 `name` |
| `date` | string | 文档日期 `YYYY-MM-DD` |
| `time` | string | 生成时间 `"HH:MM"`（引号包裹） |
| `category` | string | 分类目录名，由路由规则决定 |
| `tags` | list | 自动生成的标签：领域 + 类型 + 项目 + 关键词 |
| `source` | string | 固定 `auto-crab`，区分自动生成与手动编写 |
| `summary` | string | 一句话摘要（由 LLM 生成或留空） |
| `related` | list | 关联文档（预留字段） |

---

## 六、标签生成规则

`generate_vault_tags()` 根据分类目录和任务名称自动生成标签：

1. **领域+类型标签**：根据目录映射（如 `invest-explore` → `["投资", "日报"]`）
2. **关键词标签**：扫描任务名称，匹配关键词追加标签（如包含 `AI` → 追加 `AI`）
3. **项目标签**：始终追加 `proj:auto-crab`

标签体系与 auto-notebook 设计规范对齐：
- 领域标签：`投资`, `科技`, `创业`, `编程`, `思考`
- 类型标签：`日报`, `热点`, `灵感`, `笔记`, `复盘`
- 技术标签：`tech:python`, `tech:rust`, `tech:rag`, `tech:prompt`
- 项目标签：`proj:auto-crab`

---

## 七、代码入口

| 函数 | 文件 | 职责 |
|------|------|------|
| `resolve_vault_subdir()` | `src-tauri/src/lib.rs` | 关键词匹配 → 目录路由 |
| `generate_vault_tags()` | `src-tauri/src/lib.rs` | 根据目录和任务名生成标签列表 |
| `save_to_vault()` | `src-tauri/src/lib.rs` | 组装 frontmatter + 写文件 |
| `save_to_knowledge_base` | `src-tauri/src/commands.rs` | Tauri IPC 命令（手动保存） |
| `KnowledgeConfig` | `src-tauri/src/config/schema.rs` | 配置结构体 |
