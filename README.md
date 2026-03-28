# 🦀 Auto Crab — AI 全能生活助理

比 OpenClaw 更安全、更受控、配置更简单的桌面级 AI 助理。

支持 DeepSeek、通义千问（含 VL 视觉）、智谱 GLM、Kimi、OpenAI、Claude 以及本地 Ollama 模型。

---

## 文档索引

| 文档 | 说明 |
|------|------|
| `docs/feishu-setup.md` | 飞书远程控制完整配置 |
| `docs/life-assistant-plan.md` | 生活助理系统方案设计 |
| `docs/desktop-vision-automation-plan.md` | 桌面视觉自动化方案 |
| `TASKS.md` | 开发任务清单与进度 |

---

## 日常启动指引

### 方式一：完整启动（桌面端 + 飞书远控）

打开 **两个 PowerShell 终端**：

**终端 1 — 启动 Auto Crab：**

```powershell
# 如果 cargo 找不到，先加 PATH（永久配好后可省略）
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"

# 进入项目目录
cd C:\Workspace\aiProg\auto-crab

# 启动开发模式
pnpm tauri dev
```

看到以下日志说明启动成功：
```
INFO Auto Crab starting...
INFO Webhook server started on port 18790
INFO TaskScheduler started with 4 jobs    ← 定时任务已加载
INFO System tray initialized
```

**终端 2 — 启动 ngrok（飞书远控需要）：**

```powershell
ngrok http 18790
```

启动后会显示公网地址（如 `https://xxxx.ngrok-free.app`），把这个地址更新到飞书开放平台 → 事件订阅 → 请求地址。

> ⚠️ ngrok 免费版每次重启地址会变，需要同步更新飞书后台。

### 方式二：仅桌面端（不需要飞书）

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
cd C:\Workspace\aiProg\auto-crab
pnpm tauri dev
```

### 关机后重启检查清单

- [ ] Auto Crab 启动（`pnpm tauri dev`）
- [ ] ngrok 启动（`ngrok http 18790`）
- [ ] 飞书后台更新 ngrok 地址（如果地址变了）
- [ ] 飞书发 `/status` 验证连通性

---

## 前置条件

| 工具 | 版本 | 安装方式 |
|------|------|---------|
| Node.js | ≥ 18 | https://nodejs.org |
| pnpm | ≥ 8 | `npm install -g pnpm` |
| Rust | ≥ 1.70 | https://rustup.rs |
| VS Build Tools | 2022 | 见下方说明（仅 Windows） |

**Windows 额外步骤：** 安装 Visual Studio Build Tools：

```cmd
curl -k -L -o "%TEMP%\vs_BuildTools.exe" "https://aka.ms/vs/17/release/vs_BuildTools.exe"
"%TEMP%\vs_BuildTools.exe" --add Microsoft.VisualStudio.Workload.VCTools --add Microsoft.VisualStudio.Component.Windows11SDK.22621 --includeRecommended --passive --wait --norestart
```

### 首次安装

```bash
cd auto-crab
pnpm install          # 前端依赖
# Rust 依赖首次编译自动下载（约 3-5 分钟）
```

### 配置 PATH（重要）

**永久解决（推荐）：** 把 `%USERPROFILE%\.cargo\bin` 添加到系统环境变量 PATH 中：
- Windows 设置 → 系统 → 关于 → 高级系统设置 → 环境变量 → 用户变量 PATH → 新建 → 输入 `%USERPROFILE%\.cargo\bin`

### 生产构建

```bash
pnpm tauri build
```

构建产物在 `src-tauri/target/release/bundle/` 目录下。

---

## 首次使用

### 1. 配置模型 API Key

**方式一：通过应用 UI**
1. 打开应用 → 左侧边栏 → **设置** → **密钥管理**
2. 选择提供商名称（如 `deepseek`）
3. 粘贴 API Key → 点击"保存到密钥链"

**方式二：命令行**
```powershell
cd src-tauri
cargo run --example store_key -- deepseek sk-你的密钥
cargo run --example store_key -- dashscope sk-你的密钥
```

### 2. 配置文件

位置：`%APPDATA%\com.zelex.auto-crab\auto-crab.toml`

```toml
[models.primary]
provider = "deepseek"
model = "deepseek-chat"
api_key_ref = "keychain://deepseek"

[models.vision]
provider = "dashscope_vl"
model = "qwen3-vl-plus"
api_key_ref = "keychain://dashscope"
```

### 3. 支持的模型

| 提供商 | provider 值 | 推荐模型 | 备注 |
|--------|------------|---------|------|
| DeepSeek | `deepseek` | `deepseek-chat` | 性价比高，主力对话 |
| 通义千问 | `dashscope` | `qwen-max` | 国内速度快 |
| 通义千问 VL | `dashscope_vl` | `qwen3-vl-plus` | 视觉分析（截图/K线） |
| 智谱 GLM | `zhipu` | `glm-4` | 清华技术背景 |
| 月之暗面 | `moonshot` | `moonshot-v1-128k` | 超长上下文 |
| OpenAI | `openai` | `gpt-4o` | 需要翻墙 |
| Claude | `anthropic` | `claude-sonnet-4-20250514` | 需要翻墙 |
| Ollama | `ollama` | `qwen2.5:14b` | 本地运行 |

---

## 核心能力

### 工具列表（16 个）

| 工具 | 说明 | 风险级别 |
|------|------|---------|
| `read_file` / `list_directory` | 文件读取/列目录 | 安全 |
| `write_file` | 文件写入（自动快照） | 中风险 |
| `read_pdf` | PDF 文本提取 | 安全 |
| `execute_shell` | 命令执行 | 高风险 |
| `search_web` | 网页搜索（DuckDuckGo） | 安全 |
| `fetch_webpage` | 网页内容抓取 | 安全 |
| `get_crypto_price` | 加密货币实时价格（Binance） | 安全 |
| `screenshot` / `analyze_screen` | 截图/视觉分析 | 安全 |
| `analyze_and_act` | 截图+分析+操作一步到位 | 高风险 |
| `get_ui_tree` | Windows 控件树（<500ms） | 安全 |
| `focus_window` | 窗口聚焦 | 中风险 |
| `mouse_click` / `keyboard_type` / `key_press` | 鼠标键盘操控 | 高风险 |
| `quick_reply_wechat` | 微信快速回复 | 高风险 |

### 飞书远程指令

| 指令 | 说明 |
|------|------|
| 普通文字 | AI 对话（按用户保留上下文） |
| `/status` | 系统状态 + 模型配置 |
| `/status models` | 详细模型列表 |
| `/session 名称` | 切换到指定会话 |
| `/sessions` | 查看所有会话 |
| `/reset` | 清空当前会话 |
| `/undo` | 撤回最近一次文件修改 |
| `/monitor 60 盯BTC` | 定时监控（秒为单位） |
| `/monitors` | 查看活跃监控 |
| `/monitor stop ID` | 停止监控 |
| `/task 描述` | 创建审批任务 |
| `/approve ID` | 批准 |
| `/reject ID` | 拒绝 |

### 生活助理（定时任务）

配置在 `auto-crab.toml` 的 `[scheduled_tasks]` 中：

| 时间 | 任务 | 内容 |
|------|------|------|
| 每日 07:30 | 晨间投资简报 | 读持仓→查价格→搜新闻→生成简报→推飞书 |
| 每日 19:00 | 选题推荐 | 搜热点→读模板→生成选题→推飞书 |
| 每日 21:00 | 学习提醒 | 读计划→搜动态→简要总结→推飞书 |
| 每周日 10:00 | 周度复盘 | 读周数据→综合分析→生成周报→推飞书 |

数据目录：`~/ai-assistant-data/`（投资/职业/副业三个模块）

Prompt 模板：`~/ai-assistant-data/config/prompts/`（可随时修改，无需重启）

---

## 安全机制

| 级别 | 操作示例 | 行为 |
|------|---------|------|
| 🟢 安全 | 读文件、搜索、截图 | 自动执行 |
| 🟡 中风险 | 写文件、聚焦窗口 | 弹窗确认 |
| 🔴 高风险 | 执行命令、鼠标操控 | 弹窗确认 |
| ⛔ 禁止 | 格式化磁盘、修改引导 | 永远不允许 |

- 只读 Shell 命令（dir/ls/cat）自动通过
- write_file 前自动打快照，支持 `/undo` 撤回
- 审计日志持久化到 JSONL 文件

---

## 项目结构

```
auto-crab/
├── src/                        # 前端 (React + TypeScript + Tailwind)
│   ├── components/
│   │   ├── Chat/               # 对话界面 + 消息气泡 + 模型选择器
│   │   ├── Settings/           # 设置页面 (模型/安全/工具/远程/密钥)
│   │   ├── Sidebar/            # 侧边栏 + 历史对话
│   │   ├── TaskPanel/          # 工具执行面板（右侧实时显示）
│   │   ├── ApprovalDialog/     # 操作审批弹窗
│   │   ├── AuditLog/           # 审计日志查看器
│   │   └── Onboarding/         # 首次运行向导
│   └── stores/                 # Zustand 状态管理
├── src-tauri/                  # Rust 后端
│   ├── src/
│   │   ├── core/               # Agent 运行时 / 记忆 / 调度 / 快照 / 宏
│   │   ├── models/             # 模型适配器 (OpenAI兼容 / Ollama / 路由)
│   │   ├── security/           # 风险引擎 / 凭据 / 审计 / 审批
│   │   ├── tools/              # 文件 / Shell / 网络 / 浏览器 / UI Automation
│   │   ├── remote/             # 飞书 / 企业微信 / 审批桥接
│   │   ├── plugins/            # WASM 插件沙箱
│   │   ├── commands.rs         # Tauri IPC 命令 + 工具调度
│   │   └── lib.rs              # 应用入口 + 调度器 + 远程处理
│   ├── capabilities/           # Tauri 2.0 权限声明
│   └── defaults/               # 默认配置模板
├── docs/                       # 文档
│   ├── feishu-setup.md         # 飞书配置指南
│   ├── life-assistant-plan.md  # 生活助理方案
│   └── desktop-vision-automation-plan.md  # 视觉自动化方案
├── TASKS.md                    # 开发任务清单（100+ 任务）
└── README.md                   # 本文档
```

---

## 常见问题

**Q: 编译报错 `link.exe not found`？**
A: 安装 Visual Studio Build Tools，见上方"前置条件"。

**Q: 对话返回 `No model provider configured`？**
A: 检查配置文件 `[models.primary]` 和密钥链。重启应用。

**Q: API 返回 401 `invalid_api_key`？**
A: 密钥名称和 `api_key_ref` 要对应。如 `keychain://deepseek` 对应密钥名 `deepseek`。

**Q: 飞书消息无响应？**
A: 检查 ngrok 是否运行、飞书后台地址是否更新、`allowed_user_ids` 是否配置。看终端日志。

**Q: 桌面端聊天卡在"思考中"？**
A: 模型 API 调用中（非流式），等 10-30 秒。如果超过 2 分钟，检查终端日志。

**Q: 网络下载慢？**
A: 设置 Rust 中国镜像：`$env:RUSTUP_DIST_SERVER = "https://mirrors.ustc.edu.cn/rust-static"`

---

## 技术栈

- **前端**: React 19 + TypeScript + Tailwind CSS 4 + Zustand
- **后端**: Rust + Tauri 2.0
- **模型**: OpenAI 兼容 API (DeepSeek/通义/智谱/Kimi) + DashScope VL + Ollama
- **安全**: keyring (系统密钥链) + 四级风险引擎 + 审计日志
- **视觉**: xcap (截图) + Qwen3-VL (分析) + enigo (鼠标键盘) + Windows UI Automation
- **数据**: TOML 配置 + JSON 持久化 + JSONL 审计 + Binance API + DuckDuckGo 搜索
- **远程**: 飞书 Bot (Webhook) + ngrok 隧道
