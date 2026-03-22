# AI 全能生活助理系统方案设计文档

> 项目代号：Auto Crab Life Assistant  
> 版本：v1.0  
> 日期：2026-03-22  
> 状态：方案评审阶段

---

## 一、目标定义

### 1.1 核心目标

将 Auto Crab 从"桌面 AI 工具"升级为**日常化运行的个人智能助理系统**，覆盖三大生活场景：

| 模块 | 目标 | 交付物 |
|------|------|--------|
| 投资理财 | 每日自动产出交易信号和持仓操作建议 | 晨报、盘中提醒、收盘总结 |
| 职业成长 | 中长期学习路径规划 + 周度进度跟踪 | 学习计划、周报、行业动态摘要 |
| 副业发展 | 选题生成 + 运营复盘 + 变现/创业分析 | 内容日历、数据周报、方向分析 |

**设计原则：** 用户只做决策（看结论 → 同意/否决/调整），AI 承担全部信息采集和分析工作。

### 1.2 非目标

- 不做自动交易执行（只给建议，不代替操作）
- 不做全自动内容发布（选题和脚本由 AI 生成，发布由人控制）
- 不替代专业投资顾问或财务规划（AI 输出仅作参考）

---

## 二、现状分析

### 2.1 Auto Crab 已具备的能力

| 能力 | 状态 | 说明 |
|------|------|------|
| 多模型对话 | ✅ 可用 | OpenAI/DeepSeek/通义/智谱/Kimi/Anthropic/Ollama |
| 截图 + Vision 分析 | ✅ 可用 | xcap 截图 → DashScope VL (qwen-vl) 分析 |
| 鼠标/键盘操控 | ✅ 可用 | enigo 0.2，坐标点击 + 文本输入 + 快捷键 |
| 文件读写 | ✅ 可用 | 路径白名单 + ~ 展开 |
| Shell 命令 | ✅ 可用 | 命令白名单机制 |
| 飞书远控 | ✅ 可用 | Webhook → 对话/审批/多会话 |
| 飞书 /monitor | ✅ 可用 | 定时截图 → VL 分析 → 飞书推送 |
| 审计日志 | ✅ 可用 | JSONL 持久化 + UI 查看 |
| 风险管控 | ✅ 可用 | Safe/Moderate/Dangerous/Forbidden 四级 |

### 2.2 关键能力缺口（Gap Analysis）

| 缺口 | 影响模块 | 严重程度 | 说明 |
|------|---------|---------|------|
| **定时任务未接入主循环** | 全部 | 🔴 关键 | `TaskScheduler` 代码已写好，但 `lib.rs` 的 `run()` 中未创建实例、未启动轮询。整套日常化运行的基座缺失 |
| **search_web 未实现** | 投资消息面、行业动态 | 🔴 关键 | 工具注册表中有 `search_web`，但 `dispatch_tool` 中无匹配分支，调用会返回"未知工具"。AI 目前无法主动获取实时信息 |
| **Cron 仅支持固定 HH:MM** | 周报/月报/季报 | 🟡 中等 | `parse_simple_cron` 只提取前两个字段（分+时），忽略日/月/星期。无法表达"每周日""每月1号"等周期 |
| **无结构化本地数据管理** | 持仓跟踪、进度跟踪 | 🟡 中等 | 目前 AI 只能通过 read_file/write_file 操作 JSON/Markdown 文件。没有校验、版本管理、查询能力 |
| **桌面端无图片输入** | K线图手动上传 | 🟡 中等 | 桌面 ChatView 不支持粘贴/拖拽图片到对话框。只能通过 analyze_screen 截全屏 |
| **无 RSS/新闻聚合** | 投资消息面、行业动态 | 🟡 中等 | 即使 search_web 实现了，也是即时搜索。缺少订阅源自动聚合（如 36kr、雪球、TechCrunch RSS） |
| **推送仅限飞书** | 全部 | 🟢 低 | 没有微信/邮件/桌面通知等替代通道。对于不用飞书的用户是阻塞 |
| **Monitor 无法指定窗口** | K线图分析 | 🟢 低 | `/monitor` 截取的是全屏，如果交易软件不在前台或被遮挡，截图内容可能无效 |

### 2.3 现有架构优势

- **飞书远控已成熟**：对话/审批/多会话/监控 全链路已通，可作为日常推送的主渠道
- **Vision 管线已通**：截图 → VL 模型分析 → 文本输出的流程可直接用于 K 线读图
- **工具调用循环已跑通**：chat_stream_start 和 run_remote_chat 都支持多轮工具调用（最多 6 轮），AI 可以自主决定调什么工具
- **配置 schema 已预留 scheduled_tasks**：数据结构、配置解析、Settings UI 合并逻辑都在，只差接入

---

## 三、系统架构设计

### 3.1 整体架构

```
┌──────────────────────────────────────────────────────────────────┐
│                          用户触达层                               │
│                                                                  │
│   飞书Bot        桌面ChatView      系统通知          (未来)邮件    │
│   ┌─────┐       ┌──────────┐      ┌─────────┐     ┌─────────┐  │
│   │推送  │       │对话+图片  │      │Toast    │     │日报邮件  │  │
│   │命令  │       │手动触发   │      │提醒     │     │周报邮件  │  │
│   └──┬──┘       └────┬─────┘      └────┬────┘     └────┬────┘  │
└─────┼───────────────┼──────────────────┼───────────────┼────────┘
      │               │                  │               │
┌─────▼───────────────▼──────────────────▼───────────────▼────────┐
│                        调度中枢 (Scheduler Hub)                   │
│                                                                   │
│  TaskScheduler (增强版)                                           │
│  ┌─────────────────────────────────────────────────────────┐     │
│  │ Cron Parser (完整5字段) → Job Queue → Executor          │     │
│  │                                                         │     │
│  │ 07:30 晨间投资简报    → InvestmentBriefing pipeline     │     │
│  │ 09:00 盘中监控启动    → Monitor pipeline                │     │
│  │ 19:00 选题推荐       → ContentIdeas pipeline            │     │
│  │ 21:00 学习提醒       → LearningReminder pipeline        │     │
│  │ Sun 10:00 周报       → WeeklyReport pipeline            │     │
│  │ 1st 10:00 月报       → MonthlyReport pipeline           │     │
│  └─────────────────────────────────────────────────────────┘     │
│                                                                   │
│  EventBus (任务结果 → 推送分发)                                   │
│  ┌─────────────────────────────────────────────────────────┐     │
│  │ TaskCompleted → [飞书推送, 文件保存, 桌面通知]            │     │
│  └─────────────────────────────────────────────────────────┘     │
└───────────────────────────┬───────────────────────────────────────┘
                            │
┌───────────────────────────▼───────────────────────────────────────┐
│                      任务执行层 (Pipelines)                        │
│                                                                   │
│  每个 Pipeline = 一系列步骤（工具调用 + LLM 分析 + 文件 I/O）      │
│                                                                   │
│  ┌─────────────┐  ┌─────────────┐  ┌──────────────┐             │
│  │ 投资 Pipeline│  │ 成长 Pipeline│  │ 副业 Pipeline │             │
│  │             │  │             │  │              │             │
│  │ read_file   │  │ search_web  │  │ search_web   │             │
│  │ search_web  │  │ read_file   │  │ read_file    │             │
│  │ screenshot  │  │ write_file  │  │ write_file   │             │
│  │ analyze_vl  │  │ LLM分析     │  │ LLM分析      │             │
│  │ LLM分析     │  │             │  │              │             │
│  └─────────────┘  └─────────────┘  └──────────────┘             │
└───────────────────────────┬───────────────────────────────────────┘
                            │
┌───────────────────────────▼───────────────────────────────────────┐
│                      能力层 (Tools + Models)                      │
│                                                                   │
│  ┌─────────┐ ┌─────────┐ ┌──────────┐ ┌──────────┐ ┌─────────┐ │
│  │文件 I/O │ │Shell 执行│ │截图+Vision│ │Web Search│ │RSS/News │ │
│  └─────────┘ └─────────┘ └──────────┘ └──────────┘ └─────────┘ │
│                                                                   │
│  模型路由: Primary(对话) | Vision(读图) | Coding(技术) | Fallback  │
└───────────────────────────────────────────────────────────────────┘
                            │
┌───────────────────────────▼───────────────────────────────────────┐
│                      数据层 (Local Knowledge Base)                 │
│                                                                   │
│  ~/ai-assistant-data/                                             │
│  ├── investment/  (持仓、信号历史、日报)                            │
│  ├── career/      (学习计划、进度、周报)                            │
│  ├── side-business/ (选题池、排期、数据、收入)                      │
│  └── config/prompts/ (各场景提示词模板)                            │
└───────────────────────────────────────────────────────────────────┘
```

### 3.2 数据流（以「晨间投资简报」为例）

```
07:30 Cron 触发
  │
  ▼
TaskScheduler 创建 InvestmentBriefing 任务
  │
  ▼
run_remote_chat(action_prompt) 开始执行
  │
  ├── Round 1: LLM 决定 → 调用 read_file("~/ai-assistant-data/investment/positions.json")
  │   └── 返回持仓数据
  │
  ├── Round 2: LLM 决定 → 调用 search_web("AAPL 最新消息 2026-03-22")
  │   └── 返回搜索结果摘要
  │
  ├── Round 3: LLM 决定 → 调用 search_web("美联储 利率决议 2026-03")
  │   └── 返回宏观消息
  │
  ├── Round 4: LLM 综合分析 → 生成投资简报文本
  │   └── 调用 write_file 保存到 daily-reports/
  │
  └── Round 5: LLM 返回最终简报文本（无更多工具调用）
       │
       ▼
  EventBus 分发结果
       │
       ├── FeishuBot.send_message(user_id, 简报内容)
       ├── 桌面通知 (可选)
       └── 审计日志记录
```

---

## 四、功能开发方案

### 4.1 Phase 1：补齐基座（预计 2-3 天）

**目标：让定时任务和信息检索能力真正跑起来**

#### F-001: TaskScheduler 接入主循环

**现状：** `core/scheduler.rs` 已实现 TaskScheduler 结构体和 check_due_jobs 方法，但 `lib.rs` 的 `run()` 中从未创建实例。

**方案：**
```
lib.rs run() → setup 回调中:
  1. 加载 config
  2. 如果 scheduled_tasks.enabled:
     a. 创建 TaskScheduler 实例
     b. 启动 tokio::spawn 轮询循环 (每 30 秒 check_due_jobs)
     c. 对 due job:
        - auto_execute=true → 直接调用 run_remote_chat(job.action)
        - auto_execute=false → 推送审批到飞书
     d. 执行结果通过飞书推送给用户
```

**关键决策：**
- 定时任务的 action 内容本质上就是一段"用户指令"，直接走 run_remote_chat 管线
- 这样 AI 可以在执行过程中自主调用任何已注册工具
- 无需为每种任务编写独立的 Pipeline 代码

#### F-002: search_web 工具实现

**现状：** 注册表有定义，dispatch 无实现。

**方案选型：**

| 方案 | 优点 | 缺点 | 推荐 |
|------|------|------|------|
| A. SearXNG 自建 | 免费、隐私、可定制 | 需要 Docker 部署维护 | 开发环境 |
| B. Bing Search API | 免费额度、官方稳定 | 有月度限额 | ⭐ 首选 |
| C. Tavily API | 专为 AI Agent 设计，返回结构化 | 付费 | 最佳体验 |
| D. DuckDuckGo Lite | 完全免费 | 无官方 API，需要 scraping | 备用 |
| E. 本地 curl + 解析 | 零成本 | 解析不稳定 | 不推荐 |

**推荐方案：B（Bing） + C（Tavily）双引擎**

```
dispatch_tool 新增:
  "search_web" => {
      let query = args["query"];
      // 优先 Tavily（结构化好），fallback Bing
      let results = web_search(query, provider="tavily|bing").await;
      // 返回 top 5 结果的标题+摘要+URL
      format_search_results(results)
  }
```

**配置新增：**
```toml
[tools]
web_search_provider = "tavily"  # 或 "bing" 或 "searxng"
web_search_api_key_ref = "tavily_api_key"
```

#### F-003: Cron 解析器增强

**现状：** `parse_simple_cron` 只读取 minute + hour 两个字段，忽略 day/month/weekday。

**方案：** 引入 `cron` crate 替代手动解析

```toml
# Cargo.toml
cron = "0.12"
```

```rust
// scheduler.rs 改造
use cron::Schedule;
use std::str::FromStr;

fn is_job_due(cron_expr: &str) -> bool {
    // 将 5 字段转换为 cron crate 需要的 7 字段格式
    let full_expr = format!("0 {}", cron_expr); // 补上秒字段
    if let Ok(schedule) = Schedule::from_str(&full_expr) {
        // 检查最近一次触发时间是否在当前分钟内
        ...
    }
}
```

**效果：** 支持 `"30 7 * * *"`（每日 7:30）、`"0 10 * * 0"`（每周日 10:00）、`"0 10 1 * *"`（每月1号 10:00）。

---

### 4.2 Phase 2：投资理财模块（预计 2-3 天）

#### F-004: 持仓数据管理

**方案：** 不建数据库，用 JSON 文件 + Schema 校验

```
~/ai-assistant-data/investment/
├── positions.json         # 持仓清单 (标的/成本/数量/策略/止盈止损)
├── watchlist.json          # 关注列表
├── signals-history.jsonl   # 每条信号一行 JSON，追加写入
└── daily-reports/
    └── 2026-03-22.md       # 每日报告
```

**AI 管理方式：** 通过 system_prompt 指导 AI 用 read_file/write_file 维护这些文件。用户通过飞书对话指令更新：
- `"加仓 AAPL 100股 成本185"` → AI 自动更新 positions.json
- `"关注一下 TSLA"` → AI 自动更新 watchlist.json

#### F-005: K线图分析 Prompt 工程

**方案：** 不改代码，纯配置驱动

在 `~/ai-assistant-data/config/prompts/` 下放置专用 prompt 模板文件。定时任务的 action 中引用：

```
# scheduled_tasks.jobs[0].action 示例：
"请先读取 ~/ai-assistant-data/config/prompts/kline-analysis.md 作为分析框架，
 然后用 analyze_screen 截取当前屏幕，按照框架分析K线走势，
 最后读取 ~/ai-assistant-data/investment/positions.json 结合持仓给出操作建议"
```

这样 prompt 模板可以随时在文件中微调，无需改代码或重启应用。

#### F-006: 消息面聚合（依赖 F-002）

**方案：** 在 search_web 基础上，通过 action prompt 编排多轮搜索

```
action = """
按以下步骤执行晨间投资简报：
1. read_file ~/ai-assistant-data/investment/positions.json 获取持仓
2. 对每个持仓标的，search_web 搜索过去12小时的重要消息
3. search_web 搜索宏观经济关键词：美联储、CPI、非农、降息
4. 综合所有信息，按持仓逐一给出今日操作建议
5. write_file 保存日报到 ~/ai-assistant-data/investment/daily-reports/
6. 最后输出简报摘要（结论先行，每个标的一行建议）
"""
```

#### F-007: 盘中 Monitor 增强

**现状：** `/monitor` 已可用，但 prompt 是硬编码的通用模板。

**方案：** 将 monitor 的分析 prompt 改为可配置，从文件或参数中加载

```
/monitor 300 kline    → 加载 prompts/kline-monitor.md 作为分析框架
/monitor 600 news     → 用搜索模式，定期搜索新闻变化
/monitor 120 chat     → 加载 prompts/chat-monitor.md（微信消息监控）
```

**代码改动：** `lib.rs` 中 monitor 的 `monitor_prompt` 构建逻辑，支持从预设模板文件加载。

---

### 4.3 Phase 3：职业成长模块（预计 1-2 天）

#### F-008: 学习计划管理

**方案：** 纯文件驱动 + AI 对话维护

```
~/ai-assistant-data/career/
├── learning-plan.md        # 中长期计划（AI 初始化生成，手动微调）
├── progress-tracker.json   # 周进度打卡
│   {
│     "weeks": {
│       "2026-W12": {
│         "tasks_planned": ["读完《AI Agent Design》Ch3", "完成 LangGraph 教程"],
│         "tasks_completed": ["读完《AI Agent Design》Ch3"],
│         "notes": "LangGraph 教程太长，拆到下周"
│       }
│     }
│   }
├── skill-matrix.md         # 能力矩阵
└── weekly-reviews/
    └── W12.md
```

**AI 维护方式：** 每周日定时任务触发，AI 读取上周 progress，对比 plan，生成周报并推送。

#### F-009: 行业动态扫描（依赖 F-002）

**定时任务配置：**
```toml
[[scheduled_tasks.jobs]]
name = "AI行业日报"
cron = "0 21 * * *"
action = """
搜索今日 AI/大模型/Agent/开发者工具 领域最重要的3-5条新闻。
对每条新闻评估：与开发工程师的相关度（高/中/低），
以及对学习计划 ~/ai-assistant-data/career/learning-plan.md 的影响。
如果有高相关度新闻，建议调整学习优先级。
输出格式：一句话摘要 + 对你的影响 + 建议行动
"""
auto_execute = true
```

---

### 4.4 Phase 4：副业发展模块（预计 2-3 天）

#### F-010: 选题生成系统（依赖 F-002）

**数据结构：**
```json
// content-calendar.json
{
  "ideas": [
    {
      "id": "2026-03-22-001",
      "title": "用 Rust 写一个 AI Agent 的全过程",
      "type": "evergreen",      // 或 "trending"
      "platform": ["bilibili", "xiaohongshu"],
      "status": "idea",         // idea → scripting → filming → editing → published
      "priority": "high",
      "keywords": ["Rust", "AI Agent", "教程"],
      "created_at": "2026-03-22",
      "notes": ""
    }
  ],
  "schedule": [
    { "date": "2026-03-24", "idea_id": "2026-03-22-001", "platform": "bilibili" }
  ]
}
```

#### F-011: 视频数据复盘

**方案：** 用户手动录入（或未来对接平台 API），AI 分析趋势

```jsonl
// video-analytics.jsonl (每行一个视频)
{"id":"BV1xxx","title":"...","platform":"bilibili","published":"2026-03-15","views":12500,"likes":340,"coins":120,"comments":45,"favorites":210,"watch_duration_avg":185}
```

定时任务每周读取该文件，AI 分析：
- 哪类内容表现好 → 值得复制的要素
- 发布时间 vs 流量 → 最佳发布时间
- 标题关键词 vs 点击率 → 标题优化建议

#### F-012: 变现分析与创业方向（依赖 F-002）

月度/季度定时任务，action 引导 AI：
- 搜索 ProductHunt/IndieHackers/即刻 近期热门
- 结合用户技术栈（从 skill-matrix.md 读取）
- 评估 Auto Crab 本身的商业化路径
- SWOT 分析 + 最小行动建议

---

### 4.5 Phase 5：体验优化（预计 2-3 天）

#### F-013: 桌面端图片输入

允许用户在 ChatView 中粘贴/拖拽图片（如手动截取的K线图），前端编码为 base64，走 vision 模型分析。

#### F-014: 桌面通知

当定时任务产出结果时，除了飞书推送，还在系统托盘弹出 Windows Toast 通知。

#### F-015: 任务仪表盘

Settings 或新 Tab 页面，展示：
- 定时任务列表和下次执行时间
- 最近执行结果摘要
- 飞书连接状态

#### F-016: 指定窗口截图

Monitor 支持指定窗口标题截图，而非全屏。避免交易软件被遮挡时截到无关内容。

---

## 五、Prompt 工程策略

### 5.1 System Prompt 分层设计

```
┌────────────────────────────────────────────────────┐
│ Layer 1: 基础人格 (auto-crab.toml system_prompt)    │
│ "你是小蟹，个人AI助理，三重角色..."                    │
├────────────────────────────────────────────────────┤
│ Layer 2: 场景模板 (文件中的 prompt 模板)              │
│ prompts/kline-analysis.md                          │
│ prompts/investment-briefing.md                     │
│ prompts/content-ideas.md                           │
│ prompts/weekly-review.md                           │
├────────────────────────────────────────────────────┤
│ Layer 3: 任务指令 (scheduled_tasks.action)           │
│ 具体的步骤编排（读什么文件、搜什么关键词、输出什么格式）  │
└────────────────────────────────────────────────────┘
```

**关键设计点：**
- **Layer 1 写在 TOML 配置中**，定义 AI 的全局人格和输出规范（结论先行、标信心度、排优先级）
- **Layer 2 写在独立 .md 文件中**，可随时微调而无需改配置或重启
- **Layer 3 写在 scheduled_tasks.action 中**，编排工具调用顺序和输出格式
- 定时任务执行时，action 中可以包含 `"先读取 prompts/xxx.md 作为分析框架"` 的指令，让 AI 自行用 read_file 加载 Layer 2

### 5.2 输出格式规范

所有模块的 AI 输出统一遵循：

```
📊 [报告类型] [日期]

## 一句话结论
[最重要的一件事]

## 详细分析
[结构化内容]

## 行动建议（优先级排序）
1. 🔴 [紧急] ...
2. 🟡 [重要] ...
3. 🟢 [可选] ...

## 风险/注意事项
[需要警惕的事]
```

### 5.3 Prompt 模板管理

```
~/ai-assistant-data/config/prompts/
├── system-persona.md            # Layer 1 备份/参考
├── kline-analysis.md            # K线图技术分析框架
├── kline-monitor.md             # 盘中监控分析框架
├── investment-briefing.md       # 晨间投资简报模板
├── investment-close.md          # 收盘总结模板
├── learning-weekly-review.md    # 学习周报模板
├── content-ideas.md             # 选题生成模板
├── video-analytics-review.md    # 视频数据复盘模板
├── monetization-analysis.md     # 变现分析模板
└── startup-direction.md         # 创业方向分析模板
```

每个模板文件内包含：角色设定 + 分析框架 + 输出格式要求。AI 通过 read_file 加载后按模板执行。

---

## 六、配置方案

### 6.1 auto-crab.toml 完整配置

```toml
[general]
language = "zh-CN"
theme = "system"

[agent]
name = "小蟹"
personality = "professional"
max_context_tokens = 128000
system_prompt = """
你是一个高效的个人 AI 助理，名叫「小蟹」。你同时承担三个角色：
投资分析师（技术分析+消息面研判）、职业发展教练（AI时代转型）、副业变现顾问（程序员视角）。

输出规则：
1. 结论先行，一句话总结核心建议
2. 每个建议标注信心度 ★☆☆/★★☆/★★★
3. 多个建议按优先级排序
4. 主动使用工具获取信息（截图、搜索、读写文件），不要让用户去查
5. 所有数据文件存放在 ~/ai-assistant-data/ 下
6. 用中文回复
"""

[models]
[models.primary]
provider = "deepseek"
model = "deepseek-chat"
api_key_ref = "deepseek_api_key"

[models.vision]
provider = "dashscope_vl"
model = "qwen-vl-max"
api_key_ref = "dashscope_api_key"

[models.fallback]
provider = "zhipu"
model = "glm-4"
api_key_ref = "zhipu_api_key"

[tools]
shell_enabled = true
network_access = true
file_access = ["~/ai-assistant-data", "~/Desktop"]
web_search_provider = "tavily"       # 新增
web_search_api_key_ref = "tavily_key" # 新增

[remote]
enabled = true
[remote.feishu]
app_id = "cli_xxxxx"
app_secret_ref = "feishu_app_secret"
poll_interval_secs = 5
allowed_user_ids = ["your_open_id"]

[scheduled_tasks]
enabled = true
require_confirmation = false

# ─── 投资模块 ───
[[scheduled_tasks.jobs]]
name = "晨间投资简报"
cron = "30 7 * * *"
action = """
执行晨间投资简报流程：
1. 读取 ~/ai-assistant-data/config/prompts/investment-briefing.md 获取分析框架
2. 读取 ~/ai-assistant-data/investment/positions.json 获取持仓
3. 对每个持仓标的搜索最新消息
4. 搜索宏观经济关键词
5. 按框架生成简报，保存到 ~/ai-assistant-data/investment/daily-reports/
"""
auto_execute = true

[[scheduled_tasks.jobs]]
name = "收盘总结"
cron = "30 15 * * 1-5"
action = """
读取 ~/ai-assistant-data/config/prompts/investment-close.md，
结合今日晨间简报和最新消息，总结今日行情和持仓盈亏变化，给出明日关注点
"""
auto_execute = true

# ─── 副业模块 ───
[[scheduled_tasks.jobs]]
name = "选题推荐"
cron = "0 19 * * *"
action = """
读取 ~/ai-assistant-data/config/prompts/content-ideas.md 获取选题框架，
读取 ~/ai-assistant-data/side-business/ideas-backlog.md 了解已有选题，
搜索今日科技/AI/程序员领域热点，生成明日选题推荐
"""
auto_execute = true

# ─── 成长模块 ───
[[scheduled_tasks.jobs]]
name = "学习提醒"
cron = "0 21 * * *"
action = """
读取 ~/ai-assistant-data/career/learning-plan.md，
检查本周学习进度，推荐一个今日值得了解的技术动态
"""
auto_execute = true

[[scheduled_tasks.jobs]]
name = "周度综合复盘"
cron = "0 10 * * 0"
action = """
执行三合一周复盘：
1. 读取 ~/ai-assistant-data/config/prompts/weekly-review.md
2. 投资周报：读取本周daily-reports + signals-history
3. 学习周报：读取progress-tracker + learning-plan
4. 运营周报：读取video-analytics
5. 综合输出，保存到各自的 weekly-reviews 目录
"""
auto_execute = true

[[scheduled_tasks.jobs]]
name = "月度变现分析"
cron = "0 10 1 * *"
action = """
读取 ~/ai-assistant-data/config/prompts/monetization-analysis.md，
搜索程序员变现和独立开发最新趋势，结合 skill-matrix 分析匹配度，
生成月度变现分析报告
"""
auto_execute = true
```

---

## 七、实施路线图

```
Phase 1 (Week 1)                    Phase 2 (Week 2)
┌─────────────────────┐             ┌──────────────────────┐
│ F-001 Scheduler接入  │             │ F-004 持仓数据管理    │
│ F-002 search_web实现 │             │ F-005 K线Prompt工程   │
│ F-003 Cron增强       │             │ F-006 消息面聚合      │
│                     │             │ F-007 Monitor增强     │
│ 🎯 基座能力完整      │             │                      │
│                     │             │ 🎯 投资模块可用       │
└─────────────────────┘             └──────────────────────┘

Phase 3 (Week 3)                    Phase 4 (Week 3-4)
┌──────────────────────┐            ┌──────────────────────┐
│ F-008 学习计划管理    │            │ F-010 选题系统        │
│ F-009 行业动态扫描    │            │ F-011 视频数据复盘    │
│                      │            │ F-012 变现/创业分析   │
│ 🎯 成长模块可用       │            │                      │
│                      │            │ 🎯 副业模块可用       │
└──────────────────────┘            └──────────────────────┘

Phase 5 (Week 4-5)
┌──────────────────────┐
│ F-013 图片输入        │
│ F-014 桌面通知        │
│ F-015 任务仪表盘      │
│ F-016 窗口截图        │
│                      │
│ 🎯 体验优化完成       │
└──────────────────────┘
```

### 7.1 开发优先级排序

| 优先级 | 功能 | 依赖 | 工作量 | 价值 |
|--------|------|------|--------|------|
| P0 | F-001 Scheduler 接入 | 无 | 0.5 天 | 整套系统的基座 |
| P0 | F-002 search_web 实现 | 无 | 1 天 | 投资消息面+行业扫描的前提 |
| P0 | F-003 Cron 增强 | 无 | 0.5 天 | 周报/月报的前提 |
| P1 | F-005 K线 Prompt 工程 | F-002 | 0.5 天 | 投资核心体验（纯配置） |
| P1 | F-004 持仓数据管理 | 无 | 0.5 天 | 投资模块数据基础（纯配置） |
| P1 | F-007 Monitor 增强 | 无 | 0.5 天 | 盘中体验提升 |
| P2 | F-008 学习计划管理 | 无 | 0.5 天 | 成长模块（纯配置） |
| P2 | F-009 行业动态扫描 | F-002 | 0.5 天 | 成长模块（纯配置） |
| P2 | F-010 选题系统 | F-002 | 0.5 天 | 副业模块（纯配置） |
| P3 | F-006 消息面聚合 | F-002 | 0.5 天 | 投资模块深化 |
| P3 | F-011 视频复盘 | 无 | 0.5 天 | 副业模块（纯配置） |
| P3 | F-012 变现/创业分析 | F-002 | 0.5 天 | 副业模块（纯配置） |
| P4 | F-013 图片输入 | 无 | 1 天 | 体验优化 |
| P4 | F-014 桌面通知 | F-001 | 0.5 天 | 体验优化 |
| P4 | F-015 任务仪表盘 | F-001 | 1 天 | 体验优化 |
| P4 | F-016 窗口截图 | 无 | 1 天 | 体验优化 |

### 7.2 最小可用版本 (MVP)

**只做 F-001 + F-002 + F-003 三项代码开发**，其余全部通过 Prompt 工程 + 配置文件实现。

理由：Auto Crab 的 run_remote_chat 已经支持多轮工具调用循环，AI 能自主决定该调什么工具。所以大部分"模块"不需要写代码，只需要：
1. 一个能跑的定时器（F-001）
2. 一个能搜索的工具（F-002）
3. 一组好的 Prompt 模板（纯文件）

**MVP 开发量估算：2-3 天**

---

## 八、风险与注意事项

### 8.1 技术风险

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|---------|
| search_web 结果质量不稳定 | 中 | 消息面研判失准 | 多源交叉验证 + Tavily 结构化输出 |
| Vision 模型误读 K 线图 | 中 | 给出错误信号 | 强调仅供参考 + 多次采样取众数 |
| 定时任务 action 过长超 token | 低 | 执行失败 | 拆分为多个小任务 |
| 飞书 API 限流 | 低 | 推送延迟 | 控制推送频率 + 消息合并 |

### 8.2 业务风险

| 风险 | 缓解措施 |
|------|---------|
| 用户过度依赖 AI 投资信号 | 所有投资输出强制附加风险声明，标注信心度 |
| AI 幻觉导致虚假消息 | search_web 结果附带原始 URL，可追溯验证 |
| Prompt 模板需要反复调优 | 第一周人工审查所有输出，持续微调 |

### 8.3 成本估算（月度）

| 项目 | 用量估算 | 费用 |
|------|---------|------|
| DeepSeek-Chat | ~200 次/月 × ~2K tokens | ¥5-10 |
| Qwen-VL (Vision) | ~100 次/月 (K线分析) | ¥10-20 |
| Tavily Search | ~300 次/月 | ~$5 (免费额度1000次/月) |
| **合计** | | **¥30-60/月** |

---

## 九、快速验证计划

在全面开发前，可以先用**纯手动方式**验证核心价值：

### Day 1：手动验证投资模块
1. 打开交易软件，让 Auto Crab 截图分析（桌面对话 or 飞书 `/monitor`）
2. 评估 Vision 模型对 K 线图的分析质量
3. 手动发送 "帮我搜索 AAPL 最新消息"（等 search_web 实现后）
4. 判断：Vision 分析是否有参考价值？是否值得做自动化？

### Day 2：手动验证成长模块
1. 对话中让 AI 生成学习计划
2. 评估计划质量和可执行性
3. 判断：是否值得做自动化周复盘？

### Day 3：手动验证副业模块
1. 让 AI 搜索今日热点并生成选题
2. 评估选题相关度和创意水平
3. 判断：AI 选题是否真的能帮到内容创作？

**验证通过后**，再按 Phase 1-5 路线图推进开发。

---

## 附录 A：与 Playbook 运营手册的关系

本文档是**方案设计文档**（what to build & how），配套的 `docs/ai-life-assistant-playbook.md` 是**运营使用手册**（how to use）。

- 本文档面向开发决策，解决"做什么、怎么做、什么顺序"
- Playbook 面向日常使用，解决"怎么配置、怎么发指令、怎么看结果"
- 开发完成后，Playbook 中的配置示例和 Prompt 模板可直接投入使用
