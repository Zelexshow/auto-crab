# Auto-Crab 项目任务清单

> 桌面级自动 AI 助理，比 OpenClaw 更安全、更受控、配置更简单
> 支持本地模型 + 中国国产模型 | Windows / Linux / macOS

---

## 环境前置条件

- [x] Node.js v25.5.0 + pnpm 8.15.2
- [x] Rust 1.94.0 + Cargo 1.94.0 (中科大镜像源)
- [x] Visual Studio Build Tools 2022 (C++ 桌面开发工作负载)

---

## P0: 项目基座 (Foundation) ✅ 100%

- [x] T-001: 创建任务清单文件
- [x] T-002: 安装 Rust 工具链 (rustup) — 1.94.0 via 中科大镜像
- [x] T-003: 使用 Tauri 2.0 + React + TypeScript 初始化项目
- [x] T-004: 建立 Rust 后端目录结构 (config / core / models / security / tools / plugins / remote)
- [x] T-005: 建立前端目录结构 (components / hooks / stores)
- [x] T-006: 实现 TOML 配置系统 — `config/mod.rs` + `config/schema.rs`
- [x] T-007: 实现系统密钥链凭据管理 — `security/credentials.rs`
- [x] T-008: 实现操作风险分级引擎 — `security/risk.rs`
- [x] T-009: 实现审计日志系统 — `security/audit.rs`

## P1: 模型适配层 (Model Layer) ✅ 100%

- [x] T-010: 定义统一 ModelProvider trait — `models/provider.rs`
- [x] T-011: 实现 OpenAI 兼容适配器 — `models/openai_compat.rs`
- [x] T-012: 实现通义千问适配器 — 通过 OpenAI 兼容层
- [x] T-013: 实现 DeepSeek 适配器 — 通过 OpenAI 兼容层
- [x] T-014: 实现智谱 GLM 适配器 — 通过 OpenAI 兼容层
- [x] T-015: 实现月之暗面 Kimi 适配器 — 通过 OpenAI 兼容层
- [x] T-016: 实现 Ollama 本地模型适配器 — `models/ollama.rs`
- [x] T-017: 实现 Anthropic Claude 适配器 — 通过 OpenAI 兼容层
- [x] T-018: 实现模型智能路由 — `models/router.rs`

## P2: Agent 运行时 (Agent Runtime) ✅ 100%

- [x] T-019: 实现 Agent 核心运行时 — `core/agent.rs` (思考链 + 工具调用循环 + 审批集成)
- [x] T-020: 实现上下文管理器 — `core/context.rs` (消息管理 + token 估算 + 自动截断)
- [x] T-021: 实现文件操作工具 — `tools/file_ops.rs`
- [x] T-022: 实现 Shell 执行工具 — `tools/shell.rs`
- [x] T-023: 实现网络请求工具 — `tools/web.rs` (GET/POST + 域名白名单)
- [x] T-024: 实现操作审批门控 — `security/approval.rs`
- [x] T-025: 实现记忆系统 — `core/memory.rs`

## P3: 桌面体验 (Desktop UX) ✅ 100%

- [x] T-026: 实现主对话界面 — `Chat/ChatView.tsx` + `ChatMessage.tsx`
- [x] T-027: 实现设置界面 — `Settings/SettingsView.tsx` (5 个标签页，全部可编辑)
- [x] T-028: 实现任务监控面板 — `TaskPanel/TaskPanel.tsx`
- [x] T-029: 实现操作审批弹窗组件 — `ApprovalDialog/ApprovalDialog.tsx`
- [x] T-030: 实现模型选择器组件 — `Chat/ModelSelector.tsx`
- [x] T-031: 实现首次运行向导 — `Onboarding/OnboardingWizard.tsx` (3步向导)
- [x] T-032: 实现系统托盘 — Rust tray menu + 双击恢复窗口
- [x] T-033: 实现暗色/亮色/跟随系统主题 — CSS 变量 + data-theme

## P4: 远程控制 (Remote Control) ✅ 100%

- [x] T-034: 设计远程控制安全协议 — `remote/protocol.rs`
- [x] T-035: 实现飞书 Bot 适配器 — `remote/feishu.rs`
- [x] T-036: 实现企业微信 Bot 适配器 — `remote/wechat_work.rs`
- [x] T-037: 实现远程指令解析器 — `remote/protocol.rs`
- [x] T-038: 远程操作审批升级 — `remote/approval_bridge.rs` (推送审批到飞书/微信)

## P5: 高级功能 (Advanced) ✅ 100%

- [x] T-039: 实现 WASM 插件沙箱 — `plugins/sandbox.rs` + `plugins/manifest.rs` (权限隔离架构)
- [x] T-040: 实现浏览器自动化工具 — `tools/browser.rs` (Chrome/Edge CDP, 页面抓取/截图)
- [x] T-041: 实现可控定时任务 — `core/scheduler.rs` (cron 解析 + 确认机制)
- [x] T-042: 实现操作回滚 — `core/snapshots.rs` (文件快照 + 恢复 + 自动清理)
- [x] T-043: 实现审计日志查看器 UI — `AuditLog/AuditLogView.tsx`

## P6: 发布 (Release) — 部分完成

- [x] T-044: 配置 GitHub Actions CI/CD (Windows / macOS / Linux) — release.yml workflow
- [ ] T-045: 配置 Tauri Updater 自动更新（需要发布后配置 endpoint）
- [x] T-046: 生成各平台安装包 (msi / dmg / deb+AppImage) — CI 自动构建
- [x] T-047: 编写用户文档和 README

## P13: MCP 协议支持 (Model Context Protocol) ✅ 2026-03-31

- [x] T-095: MCP Client — 连接外部 MCP Server 扩展工具能力（rmcp crate, stdio 传输, 自动发现/路由）
- [x] T-096: MCP Server — 暴露 Auto-Crab 工具给外部 AI（search_web, get_market_price, read_file, fetch_webpage, execute_shell）
- [x] T-097: MCP 配置 schema（McpConfig + McpServerEntry, TOML 配置, Tauri 命令）
- [x] T-098: MCP 引擎集成（AgentEngine 工具路由, 命名空间化, 安全级别）
- [x] T-099: MCP Server CLI 模式（--mcp-server 标志, 可被 Cursor/Claude Desktop 连接）

## P7: 能力联通 (Tool Execution) ✅ 2026-03-21

- [x] T-048: 将 Agent 工具调用循环真正接入 chat_stream_start（文件/Shell 真实执行，agent-step 事件推送）
- [x] T-049: SettingsView 挂载时加载实际 TOML 配置，底部增加"保存配置"按钮
- [x] T-050: 修复 CSP 允许 http://localhost（Ollama/本地模型）
- [x] T-051: 添加 Tauri 2.0 capabilities/default.json
- [x] T-052: 飞书远程会话上下文记忆（按用户维度持续对话 + /reset 指令）
- [x] T-053: 飞书审批状态机接入（/task → ApprovalState → /approve 真实执行 → 结果回传）
- [x] T-054: 完善飞书远程配置文档（docs/feishu-setup.md，含后台 checklist 和验收流程）

## P8: 安全管控 + 屏幕能力 (Security & Vision) — 2026-03-21

- [x] T-055: 审计日志 get_audit_log 接入真实 AuditLogger（内存缓存 + JSONL 持久化）
- [x] T-056: 工具调用增加审计记录（dispatch_tool_with_audit，每次执行自动写审计日志）
- [x] T-057: 工具调用前风险评估（RiskEngine 自动判断 Safe/Moderate/Dangerous/Forbidden）
- [x] T-058: 新增截图工具 screenshot（xcap crate，保存 PNG，注册到工具列表）
- [x] T-059: 文件操作路径 ~ 展开（shellexpand，Windows 兼容）
- [x] T-060: Shell 白名单扩展（cmd/powershell/echo/dir/mkdir 等常用系统命令）
- [x] T-061: 系统提示词增加 OS 上下文（Windows 路径格式、桌面路径、工具使用引导）
- [x] T-062: 新增 Qwen2.5-VL-Plus 多模态视觉模型支持（dashscope_vl provider）
- [x] T-063: Settings 模型配置显示 API Key 状态（已配置/未配置，绿/红指示灯）
- [x] T-064: Settings 密钥管理显示已保存密钥列表（8 个常用密钥名的存在状态）
- [x] T-065: Settings 保存配置改为"先读后写"合并模式（不覆盖 system_prompt、飞书白名单等）
- [x] T-066: 新增 check_credentials 后端命令（批量检查密钥存在性）

## P9: 交互操控 + 多会话 + UI 优化 — 2026-03-21

- [x] T-067: 鼠标点击工具 mouse_click（enigo 0.2，坐标点击，左/右/双击）
- [x] T-068: 键盘输入工具 keyboard_type（enigo 0.2，文本输入）
- [x] T-069: 按键组合工具 key_press（ctrl+c/alt+tab/enter 等快捷键）
- [x] T-070: 飞书多会话支持（/session 切换、/sessions 列表、独立上下文）
- [x] T-071: Settings 模型名下拉（按 provider 自动推荐模型列表）
- [x] T-072: Settings API Key 状态显示部分值（如 sk-ae****dd52）
- [x] T-073: Settings 密钥管理列表显示部分值 + 编辑/添加按钮
- [x] T-074: 新增 get_credential_preview 后端命令（获取密钥脱敏预览）

## P10: 路由 + 监控 — 2026-03-21

- [x] T-075: 新增 models.vision 配置槽位（独立于 coding，专用于视觉模型）
- [x] T-076: /status 增强（显示模型配置、工具状态、用法提示）
- [x] T-077: /status models 子命令（查看所有模型详细配置）
- [x] T-078: 定时监控 /monitor <间隔> <描述>（定时截图→VL分析→飞书推送）
- [x] T-079: /monitors 查看活跃监控任务列表
- [x] T-080: /monitor stop <ID> 停止监控
- [x] T-081: 监控任务后台 tokio 协程 + watch channel 取消机制

## P11: 安全加固 + 工具补齐 — 2026-03-21

- [x] T-082: 桌面端工具调用风险拦截（Moderate/Dangerous 弹审批窗，Forbidden 直接拒绝）
- [x] T-083: 审批确认通道（DESKTOP_APPROVALS 全局 oneshot channel，approve/reject 解锁执行）
- [x] T-084: write_file 前自动快照（SnapshotStore 集成，已有文件自动备份）
- [x] T-085: /undo 命令撤回最近一次文件修改
- [x] T-086: Settings 文件目录选择器（Tauri dialog 插件，系统对话框选择文件夹）
- [x] T-087: 网页抓取工具 fetch_webpage（HTTP GET + HTML 去标签，用于文献/数据归纳）
- [x] T-088: 桌面端 TaskPanel 显示工具调用过程（右侧面板，实时 agent-step 事件）

## P12: Agent 智能升级 — 2026-03-28

- [x] T-089: search_web 多引擎 fallback（Bing 中国可用优先 + DuckDuckGo 备选）
- [x] T-090: 企业微信 Bot 接入（webhook_server 路由 + AES-CBC 消息解密 + 回复链路）
- [x] T-091: 任务规划器 Planner（core/planner.rs, 复杂任务自动分解 + 反思 + 步骤控制）
- [x] T-092: AgentEngine 集成 Planner（should_plan 启发式检测 + run_with_plan 多步执行）
- [x] T-093: TaskPanel 展示任务计划（前端 plan 类型步骤渲染）
- [x] T-094: GitHub Actions CI/CD（release.yml, Windows/macOS/Linux 三平台自动构建）

---

## 卡点记录

- ⚠️ 屏幕操控窗口焦点问题: 操作时目标窗口可能失去焦点（飞书通知抢焦点），需要在 mouse_click 前用 Windows API（SetForegroundWindow）强制聚焦目标窗口，操作后截图验证结果，失败自动重试。
- ⚠️ T-039 WASM 沙箱: 架构和权限系统已完成，但 wasmtime 运行时未集成（需要在 Cargo.toml 添加 `wasmtime` 依赖并实现 call() 方法）。当前 `sandbox.call()` 返回占位错误。添加 wasmtime 后需约 2-3 小时接入。
- ⚠️ T-040 浏览器自动化: 基于 headless Chrome `--dump-dom` 和 `--screenshot`，依赖系统已安装 Chrome/Edge。更高级的交互式操作（点击、填表）需要引入 `chromiumoxide` 或 `playwright` crate。

---

## 进度日志

| 日期 | 完成任务 | 备注 |
|------|---------|------|
| 2026-03-14 | T-001 ~ T-018, T-021~T-022, T-024 | 项目骨架 + 核心模块代码全部创建 |
| 2026-03-14 | 环境搭建完成 | VS Build Tools 安装，cargo check 通过，应用启动 |
| 2026-03-14 | T-019~T-020, T-025, T-029 | Agent 运行时 + 上下文管理 + 记忆持久化 + 审批弹窗 |
| 2026-03-15 | T-023, T-030, T-032, UI美化 | 网络请求工具 + 模型选择器 + 系统托盘 + 对话框美化 |
| 2026-03-15 | T-028, T-031, T-043 | 任务监控面板 + 首次运行向导 + 审计日志查看器 |
| 2026-03-15 | T-038~T-042 | 远程审批升级 + WASM沙箱 + 浏览器自动化 + 定时任务 + 操作回滚 |
| 2026-03-21 | T-048~T-054 | Agent工具链接通 + 飞书上下文记忆 + 审批状态机 + Settings真实配置 |
| 2026-03-21 | T-055~T-061 | 审计日志接通 + 风险评估 + 截图工具 + Shell白名单 + 路径修复 |
| 2026-03-21 | T-062~T-066 | Qwen-VL多模态 + Settings密钥状态 + 配置合并保存 |
| 2026-03-21 | T-067~T-074 | 鼠标键盘操控 + 飞书多会话 + Settings下拉/密钥预览 |
| 2026-03-21 | T-075~T-081 | Vision槽位 + 定时监控 + /status增强 |
| 2026-03-28 | T-089~T-094 | 搜索多引擎 + 企业微信接入 + 任务规划器 + CI/CD |
| 2026-03-31 | T-095~T-099 | MCP 协议支持（Client + Server + 配置 + 引擎集成 + CLI） |

## 里程碑

- ✅ 2026-03-14: 项目创建，环境搭建，核心架构完成
- ✅ 2026-03-15: P0~P5 全部完成（47个任务中45个完成，2个有卡点已记录）
- ✅ 2026-03-21: P7 能力联通（工具链真实执行 + 飞书完整远控闭环）
- ✅ 2026-03-21: P8 安全管控 + 屏幕能力（审计日志 + 截图 + VL模型 + Settings完善）
- ✅ 2026-03-21: P9 交互操控 + 多会话 + UI 优化
- ✅ 2026-03-21: P10 路由 + 定时监控
- ✅ 2026-03-21: P11 安全加固 + 工具补齐（审批拦截 + 快照 + 目录选择 + 网页抓取 + TaskPanel）
- ✅ 2026-03-28: P12 Agent 智能升级（搜索多引擎 + 企业微信 + 任务规划器 + CI/CD）
- ✅ 2026-03-31: P13 MCP 协议支持
- 🔲 下一步: T-045 Tauri Updater 配置
