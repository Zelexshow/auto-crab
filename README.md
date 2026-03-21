# 🦀 Auto Crab — 安全桌面 AI 助理

比 OpenClaw 更安全、更受控、配置更简单的桌面级 AI 助理。

支持通义千问、DeepSeek、智谱 GLM、Kimi、OpenAI、Claude 以及本地 Ollama 模型。

---

## 快速启动

### 前置条件

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

### 安装依赖

```bash
cd auto-crab

# 安装前端依赖
pnpm install

# Rust 依赖会在首次编译时自动下载
```

### 配置 PATH（重要）

Rust 安装后 `cargo` 在 `%USERPROFILE%\.cargo\bin` 目录下，新开的终端可能找不到它，会报 `program not found` 错误。

**永久解决（推荐）：** 把 `%USERPROFILE%\.cargo\bin` 添加到系统环境变量 PATH 中：
- Windows 设置 → 系统 → 关于 → 高级系统设置 → 环境变量 → 用户变量 PATH → 新建 → 输入 `%USERPROFILE%\.cargo\bin`

**临时解决：** 每次开终端先执行一次：

```cmd
:: CMD 终端
set PATH=%USERPROFILE%\.cargo\bin;%PATH%
```

```powershell
# PowerShell 终端
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
```

### 开发模式启动

```bash
pnpm tauri dev
```

首次编译约 3-5 分钟（下载 + 编译 480+ 个 Rust crate），后续增量编译很快。
启动后会弹出桌面应用窗口。

### 生产构建

```bash
pnpm tauri build
```

构建产物在 `src-tauri/target/release/bundle/` 目录下。

---

## 首次使用

### 1. 配置模型 API Key

应用首次启动会弹出向导，也可以手动配置：

**方式一：通过应用 UI**

1. 打开应用 → 左侧边栏 → **设置**
2. 点击 **密钥管理** 标签
3. 密钥名称选择对应的提供商（如 `deepseek`）
4. 粘贴你的 API Key
5. 点击 **"保存到密钥链"**

**方式二：编辑配置文件**

配置文件位置：
- Windows: `%APPDATA%\com.zelex.auto-crab\auto-crab.toml`
- macOS: `~/Library/Application Support/com.zelex.auto-crab/auto-crab.toml`
- Linux: `~/.config/com.zelex.auto-crab/auto-crab.toml`

```toml
[models.primary]
provider = "deepseek"          # 可选: dashscope / deepseek / zhipu / moonshot / openai / anthropic / ollama
model = "deepseek-chat"        # 模型名称
api_key_ref = "keychain://deepseek"  # 引用密钥链中的 key
```

### 2. 支持的模型提供商

| 提供商 | provider 值 | 推荐模型 | 备注 |
|--------|------------|---------|------|
| 通义千问 | `dashscope` | `qwen-max` | 阿里云，国内速度快 |
| DeepSeek | `deepseek` | `deepseek-chat` | 性价比高 |
| 智谱 GLM | `zhipu` | `glm-4` | 清华技术背景 |
| 月之暗面 | `moonshot` | `moonshot-v1-128k` | 超长上下文 |
| OpenAI | `openai` | `gpt-4o` | 需要翻墙 |
| Claude | `anthropic` | `claude-sonnet-4-20250514` | 需要翻墙 |
| Ollama | `ollama` | `qwen2.5:14b` | 本地运行，无需 API Key |

**使用 Ollama（本地模型）：**

```bash
# 安装 Ollama 后
ollama pull qwen2.5:14b
ollama serve
```

配置文件：

```toml
[models.primary]
provider = "ollama"
model = "qwen2.5:14b"
endpoint = "http://localhost:11434"
```

---

## 功能说明

### 对话

- 支持 Markdown 渲染、代码高亮
- 流式输出（打字机效果）
- 对话历史自动保存，重启后可恢复
- 顶栏可切换模型和主题（亮色/暗色/跟随系统）

### 安全机制

Auto Crab 的核心差异化——四级操作风险管控：

| 级别 | 颜色 | 操作示例 | 行为 |
|------|------|---------|------|
| 安全 | 🟢 | 读文件、搜索 | 自动执行 |
| 中风险 | 🟡 | 写文件、Git 提交 | 弹窗确认 |
| 高风险 | 🔴 | 执行命令、删除文件 | 密码二次验证 |
| 禁止 | ⛔ | 格式化磁盘、修改引导 | 永远不允许 |

### 远程控制

通过飞书或企业微信远程控制桌面端 AI 助理：

```toml
[remote]
enabled = true

[remote.feishu]
app_id = "cli_xxxxxxxx"
app_secret_ref = "keychain://feishu-secret"
poll_interval_secs = 30
allowed_user_ids = ["user_id_1"]
```

远程指令格式：
- 直接发文字 → AI 对话
- `/status` → 查询状态
- `/task 任务描述` → 创建任务
- `/approve ID` → 批准操作
- `/reject ID` → 拒绝操作

### 系统托盘

- 应用最小化后驻留在系统托盘
- 双击托盘图标恢复窗口
- 右键托盘菜单：显示窗口 / 退出

---

## 项目结构

```
auto-crab/
├── src/                        # 前端 (React + TypeScript + Tailwind)
│   ├── components/
│   │   ├── Chat/               # 对话界面 + 消息气泡 + 模型选择器
│   │   ├── Settings/           # 设置页面 (5个标签页)
│   │   ├── Sidebar/            # 侧边栏 + 历史对话
│   │   ├── ApprovalDialog/     # 操作审批弹窗
│   │   ├── AuditLog/           # 审计日志查看器
│   │   ├── Onboarding/         # 首次运行向导
│   │   └── TaskPanel/          # 任务监控面板
│   └── stores/                 # Zustand 状态管理
├── src-tauri/                  # Rust 后端
│   ├── src/
│   │   ├── core/               # Agent 运行时 / 上下文 / 记忆 / 调度 / 快照
│   │   ├── models/             # 模型适配器 (OpenAI兼容 / Ollama / 路由)
│   │   ├── security/           # 风险引擎 / 凭据 / 审计 / 审批
│   │   ├── tools/              # 文件 / Shell / 网络 / 浏览器
│   │   ├── remote/             # 飞书 / 企业微信 / 审批桥接
│   │   ├── plugins/            # WASM 插件沙箱
│   │   ├── commands.rs         # Tauri IPC 命令
│   │   └── lib.rs              # 应用入口
│   └── defaults/               # 默认配置模板
├── TASKS.md                    # 开发任务清单
└── README.md                   # 本文档
```

---

## 常见问题

**Q: 编译报错 `link.exe not found`？**
A: 安装 Visual Studio Build Tools，见上方"前置条件"。

**Q: 编译报错 `can't find crate for core`？**
A: 运行 `rustup component remove rust-std && rustup component add rust-std`。

**Q: 网络下载慢？**
A: 设置 Rust 中国镜像源：
```bash
export RUSTUP_DIST_SERVER=https://mirrors.ustc.edu.cn/rust-static
```

**Q: 对话返回 `No model provider configured`？**
A: 配置文件中 `[models.primary]` 未设置。参考上方"首次使用"。

**Q: API 返回 401 `invalid_api_key`？**
A: 密钥名称和配置文件中的 `api_key_ref` 要对应。如配置写 `keychain://deepseek`，则密钥名称必须是 `deepseek`。

---

## 技术栈

- **前端**: React 19 + TypeScript + Tailwind CSS 4 + Zustand
- **后端**: Rust + Tauri 2.0
- **模型**: OpenAI 兼容 API + Ollama
- **安全**: keyring (系统密钥链) + AES-GCM + 四级风险引擎
- **存储**: TOML 配置 + JSON 对话持久化 + JSONL 审计日志
