# Auto-Crab 项目任务清单

> 桌面级自动 AI 助理，比 OpenClaw 更安全、更受控、配置更简单
> 支持本地模型 + 中国国产模型 | Windows / Linux / macOS

---

## 环境前置条件

- [x] Node.js v25.5.0 + pnpm 8.15.2
- [x] Rust 1.94.0 + Cargo 1.94.0 (中科大镜像源)
- [x] Visual Studio Build Tools 2022 (C++ 桌面开发工作负载)

---

## P0: 项目基座 (Foundation)

- [x] T-001: 创建任务清单文件
- [x] T-002: 安装 Rust 工具链 (rustup) — 1.94.0 via 中科大镜像
- [x] T-003: 使用 Tauri 2.0 + React + TypeScript 初始化项目
- [x] T-004: 建立 Rust 后端目录结构 (config / models / security / tools / remote)
- [x] T-005: 建立前端目录结构 (components / hooks / stores)
- [x] T-006: 实现 TOML 配置系统 (读取/写入/默认值/验证) — `config/mod.rs` + `config/schema.rs`
- [x] T-007: 实现系统密钥链凭据管理 — `security/credentials.rs` (keyring crate)
- [x] T-008: 实现操作风险分级引擎 — `security/risk.rs` (safe/moderate/dangerous/forbidden)
- [x] T-009: 实现审计日志系统 — `security/audit.rs` (JSONL 按日期分文件)

## P1: 模型适配层 (Model Layer)

- [x] T-010: 定义统一 ModelProvider trait — `models/provider.rs` (chat / stream / tools)
- [x] T-011: 实现 OpenAI 兼容适配器 — `models/openai_compat.rs` (支持所有 OpenAI 兼容 API)
- [x] T-012: 实现通义千问适配器 — 通过 OpenAI 兼容层 (DashScope compatible-mode)
- [x] T-013: 实现 DeepSeek 适配器 — 通过 OpenAI 兼容层
- [x] T-014: 实现智谱 GLM 适配器 — 通过 OpenAI 兼容层
- [x] T-015: 实现月之暗面 Kimi 适配器 — 通过 OpenAI 兼容层
- [x] T-016: 实现 Ollama 本地模型适配器 — `models/ollama.rs`
- [x] T-017: 实现 Anthropic Claude 适配器 — 通过 OpenAI 兼容层
- [x] T-018: 实现模型智能路由 — `models/router.rs` (任务路由 + 故障回退)

## P2: Agent 运行时 (Agent Runtime) — 部分完成

- [ ] T-019: 实现 Agent 核心运行时 (思考链 + 工具调用循环)
- [ ] T-020: 实现上下文管理器 (token 计数 / 截断 / 摘要压缩)
- [x] T-021: 实现文件操作工具 — `tools/file_ops.rs` (读/写/列表/删除, 白名单)
- [x] T-022: 实现 Shell 执行工具 — `tools/shell.rs` (命令白名单 + 超时)
- [ ] T-023: 实现网络请求工具 (域名白名单)
- [x] T-024: 实现操作审批门控 — `security/approval.rs` (PendingApproval + oneshot channel)
- [ ] T-025: 实现记忆系统 (对话历史持久化 + 语义检索)

## P3: 桌面体验 (Desktop UX) — 部分完成

- [x] T-026: 实现主对话界面 — `components/Chat/ChatView.tsx` + `ChatMessage.tsx`
- [x] T-027: 实现设置界面 — `components/Settings/SettingsView.tsx` (5 个标签页)
- [ ] T-028: 实现任务监控面板 (实时显示 Agent 思考链和操作)
- [ ] T-029: 实现操作审批弹窗组件
- [ ] T-030: 实现模型选择器组件
- [ ] T-031: 实现首次运行向导 (Onboarding Wizard)
- [ ] T-032: 实现系统托盘 + 全局快捷键
- [x] T-033: 实现暗色/亮色/跟随系统主题 — CSS 变量 + prefers-color-scheme

## P4: 远程控制 (Remote Control) — 部分完成

- [x] T-034: 设计远程控制安全协议 — `remote/protocol.rs` (指令解析 + 用户白名单)
- [x] T-035: 实现飞书 Bot 适配器 — `remote/feishu.rs` (Token 管理 + 消息收发)
- [x] T-036: 实现企业微信 Bot 适配器 — `remote/wechat_work.rs`
- [x] T-037: 实现远程指令解析器 — 包含在 `remote/protocol.rs`
- [ ] T-038: 远程操作的审批升级 (危险操作推送确认到手机)

## P5: 高级功能 (Advanced)

- [ ] T-039: 实现 WASM 插件沙箱 (wasmtime 宿主)
- [ ] T-040: 实现浏览器自动化工具 (Playwright/Chrome DevTools Protocol)
- [ ] T-041: 实现可控定时任务 (cron + 执行前确认)
- [ ] T-042: 实现操作回滚 (文件修改快照 + undo)
- [ ] T-043: 实现审计日志查看器 UI

## P6: 发布 (Release)

- [ ] T-044: 配置 GitHub Actions CI/CD (Windows / macOS / Linux)
- [ ] T-045: 配置 Tauri Updater 自动更新
- [ ] T-046: 生成各平台安装包 (msi / dmg / deb+AppImage)
- [ ] T-047: 编写用户文档和 README

---

## 进度日志

| 日期 | 完成任务 | 备注 |
|------|---------|------|
| 2026-03-14 | T-001 ~ T-018, T-021~T-022, T-024, T-026~T-027, T-033~T-037 | 项目骨架 + 核心模块代码全部创建 |
| 2026-03-14 | 环境搭建完成 | VS Build Tools 安装，cargo check 通过，pnpm tauri dev 应用成功启动 |

## 里程碑

- ✅ 2026-03-14: VS Build Tools 安装完成，`cargo check` 零错误通过，`pnpm tauri dev` 应用成功启动
