# Claude Code 与 LiteLLM 代理：原理及区别

## 概述

Claude Code 支持两种 API 调用方式：**直连官方 API** 和 **通过 LiteLLM 代理转发**。两者最终调用的都是 Anthropic 的 Claude 模型，区别在于请求的路径和管理方式不同。

---

## 架构对比

### 原生 Claude Code（直连模式）

```mermaid
graph LR
    A["Claude Code<br/>客户端"] -->|"Anthropic API Key<br/>直接请求"| B["Anthropic API<br/>api.anthropic.com"]
    B --> C["Claude 模型<br/>Opus / Sonnet / Haiku"]
    C -->|"模型响应"| B
    B -->|"返回结果"| A

    style A fill:#4A90D9,stroke:#333,color:#fff
    style B fill:#D4A843,stroke:#333,color:#fff
    style C fill:#50C878,stroke:#333,color:#fff
```

### 通过 LiteLLM 代理

```mermaid
graph LR
    A["Claude Code<br/>客户端"] -->|"代理 API Key<br/>请求到代理"| P["LiteLLM 代理<br/>devai-litellm.houtai.io"]
    P -->|"路由 & 转发"| B1["Anthropic API"]
    P -->|"路由 & 转发"| B2["AWS Bedrock"]
    P -->|"路由 & 转发"| B3["Azure / 其他"]
    B1 --> C["Claude 模型"]
    B2 --> C
    B3 --> C
    C -->|"模型响应"| P
    P -->|"返回结果"| A

    style A fill:#4A90D9,stroke:#333,color:#fff
    style P fill:#E74C3C,stroke:#333,color:#fff
    style B1 fill:#D4A843,stroke:#333,color:#000
    style B2 fill:#D4A843,stroke:#333,color:#000
    style B3 fill:#D4A843,stroke:#333,color:#000
    style C fill:#50C878,stroke:#333,color:#fff
```

---

## LiteLLM 是什么

[LiteLLM](https://github.com/BerriAI/litellm) 是一个开源的 **大模型 API 代理网关**，核心功能是将 100+ 种大模型 API（Anthropic、OpenAI、Azure、AWS Bedrock、Google Vertex 等）统一为 **OpenAI 兼容格式**。

它本身 **不是大模型**，只是一个请求转发和管理的中间层。

```mermaid
graph TB
    subgraph 客户端
        C1["Claude Code"]
        C2["ChatGPT 兼容客户端"]
        C3["自定义应用"]
    end

    subgraph LiteLLM 代理层
        LLM["LiteLLM Gateway"]
        LLM --- F1["🔑 密钥管理"]
        LLM --- F2["📊 用量统计"]
        LLM --- F3["⚖️ 负载均衡"]
        LLM --- F4["🚦 限流控制"]
        LLM --- F5["📝 审计日志"]
    end

    subgraph 模型提供商
        M1["Anthropic<br/>Claude 系列"]
        M2["OpenAI<br/>GPT 系列"]
        M3["AWS Bedrock<br/>多模型"]
        M4["Azure OpenAI"]
        M5["Google Vertex AI"]
    end

    C1 --> LLM
    C2 --> LLM
    C3 --> LLM
    LLM --> M1
    LLM --> M2
    LLM --> M3
    LLM --> M4
    LLM --> M5

    style LLM fill:#E74C3C,stroke:#333,color:#fff
    style M1 fill:#50C878,stroke:#333,color:#fff
    style M2 fill:#50C878,stroke:#333,color:#fff
    style M3 fill:#50C878,stroke:#333,color:#fff
    style M4 fill:#50C878,stroke:#333,color:#fff
    style M5 fill:#50C878,stroke:#333,color:#fff
```

---

## 请求流程对比

### 直连模式

```mermaid
sequenceDiagram
    participant CC as Claude Code
    participant API as Anthropic API

    CC->>CC: 读取 ANTHROPIC_API_KEY
    CC->>API: POST /v1/messages<br/>Authorization: Bearer sk-ant-xxx
    API->>API: 验证 Key + 调用 Claude 模型
    API-->>CC: 返回模型响应（流式）
```

### 代理模式

```mermaid
sequenceDiagram
    participant CC as Claude Code
    participant Proxy as LiteLLM 代理
    participant API as Anthropic / Bedrock

    CC->>CC: 执行 apiKeyHelper 获取 Key
    CC->>Proxy: POST /v1/messages<br/>model: global.anthropic.claude-opus-4-6-v1
    Proxy->>Proxy: 验证代理 Key
    Proxy->>Proxy: 解析模型路由前缀<br/>"global.anthropic." → Anthropic
    Proxy->>API: 转发请求（使用后端真实 Key）
    API->>API: 调用 Claude Opus 4.6
    API-->>Proxy: 返回模型响应
    Proxy->>Proxy: 记录用量 & 计费
    Proxy-->>CC: 透传模型响应（流式）
```

---

## 关键配置项解读

以当前 `~/.claude/settings.json` 配置为例：

```json
{
    "apiKeyHelper": "echo sk-xxxxx",
    "env": {
        "ANTHROPIC_MODEL": "global.anthropic.claude-opus-4-6-v1",
        "ANTHROPIC_BASE_URL": "https://devai-litellm.houtai.io/",
        "CLAUDE_CODE_SUBAGENT_MODEL": "global.anthropic.claude-opus-4-6-v1",
        "DISABLE_TELEMETRY": "1"
    }
}
```

| 配置项 | 作用 |
|--------|------|
| `apiKeyHelper` | 动态获取 API Key 的命令，代替静态写死 Key |
| `ANTHROPIC_BASE_URL` | 替换默认的 `api.anthropic.com`，指向 LiteLLM 代理 |
| `ANTHROPIC_MODEL` | 模型标识，带 `global.anthropic.` 前缀用于 LiteLLM 路由 |
| `CLAUDE_CODE_SUBAGENT_MODEL` | 子代理使用的模型 |
| `DISABLE_TELEMETRY` | 禁止 Claude Code 向 Anthropic 上报遥测数据 |

### 模型名前缀含义

```
global.anthropic.claude-opus-4-6-v1
│       │          └── 模型名称（Claude Opus 4.6）
│       └── 提供商标识（Anthropic）
└── LiteLLM 路由策略（全局路由）
```

---

## 详细对比

| 维度 | 原生直连 | LiteLLM 代理 |
|------|---------|-------------|
| **API 端点** | `api.anthropic.com` | 自定义代理地址 |
| **认证方式** | Anthropic 官方 API Key 或账号登录 | 代理分配的 Key（`apiKeyHelper`） |
| **调用的模型** | Claude 原版 | Claude 原版（代理只转发） |
| **模型效果** | ✅ 无损 | ✅ 无损 |
| **计费方** | Anthropic 直接计费 | 组织/团队统一计费 |
| **网络延迟** | 一跳直达 | 多一跳（通常 < 50ms） |
| **Key 管理** | 个人自行管理 | 组织统一分发、轮换 |
| **用量监控** | Anthropic Console | LiteLLM Dashboard + 自定义告警 |
| **多模型切换** | 需改配置 | 改模型前缀即可路由 |
| **地域限制** | 受 Anthropic 区域策略影响 | 代理服务器中转可绕过 |
| **适用场景** | 个人开发者 | 企业/团队协作 |

---

## 优劣势分析

### 直连模式

```mermaid
graph TB
    subgraph "✅ 优势"
        A1["延迟最低<br/>一跳直达"]
        A2["配置简单<br/>开箱即用"]
        A3["功能完整<br/>官方全量支持"]
    end
    subgraph "❌ 劣势"
        B1["需个人 API Key<br/>个人承担费用"]
        B2["地域限制<br/>部分地区不可用"]
        B3["无法统一管控<br/>团队管理困难"]
    end

    style A1 fill:#27AE60,stroke:#333,color:#fff
    style A2 fill:#27AE60,stroke:#333,color:#fff
    style A3 fill:#27AE60,stroke:#333,color:#fff
    style B1 fill:#E74C3C,stroke:#333,color:#fff
    style B2 fill:#E74C3C,stroke:#333,color:#fff
    style B3 fill:#E74C3C,stroke:#333,color:#fff
```

### LiteLLM 代理模式

```mermaid
graph TB
    subgraph "✅ 优势"
        A1["统一计费<br/>组织承担成本"]
        A2["Key 安全<br/>后端 Key 不暴露给用户"]
        A3["多提供商<br/>灵活切换 Bedrock/Azure"]
        A4["绕过地域限制<br/>代理服务器中转"]
        A5["用量审计<br/>精细化监控每人消耗"]
    end
    subgraph "❌ 劣势"
        B1["额外延迟<br/>多一跳网络开销"]
        B2["依赖代理可用性<br/>代理挂了全团队受影响"]
        B3["部署维护成本<br/>需运维 LiteLLM 服务"]
    end

    style A1 fill:#27AE60,stroke:#333,color:#fff
    style A2 fill:#27AE60,stroke:#333,color:#fff
    style A3 fill:#27AE60,stroke:#333,color:#fff
    style A4 fill:#27AE60,stroke:#333,color:#fff
    style A5 fill:#27AE60,stroke:#333,color:#fff
    style B1 fill:#E74C3C,stroke:#333,color:#fff
    style B2 fill:#E74C3C,stroke:#333,color:#fff
    style B3 fill:#E74C3C,stroke:#333,color:#fff
```

---

## 故障排查实录：代理超时问题

### 现象

在本地电脑运行 Claude Code，发送任何消息后持续报错：

```
Request timed out.
Retrying in 26 seconds… (attempt 7/10) · API_TIMEOUT_MS=600000ms, try increasing it
```

即使将超时设到 10 分钟（600000ms），依然无法获得响应。

### 诊断过程

#### 1. DNS 解析检查

```bash
# 解析域名
Resolve-DnsName devai-litellm.houtai.io
```

结果：

| 记录类型 | 值 |
|---------|-----|
| CNAME | `vpce-xxx.ap-southeast-1.vpce.amazonaws.com` |
| A | `10.102.255.29` |
| A | `10.102.255.43` |
| A | `10.102.255.7` |

> **关键发现：** 域名解析到了 `10.x.x.x` 的内网地址，且 CNAME 指向 AWS VPC Endpoint（`vpce-` 前缀）。

#### 2. TCP 端口连通性

```bash
Test-NetConnection -ComputerName "devai-litellm.houtai.io" -Port 443
```

| 检测项 | 结果 |
|--------|------|
| TCP 443 | ✅ 连通（TCP 握手成功） |
| Ping | ❌ 不通 |

> TCP 能通是因为 DNS 解析到了内网 IP，本地网络栈能完成 TCP 握手，但实际 HTTPS 请求无法到达真正的服务。

#### 3. HTTPS 请求测试

```bash
# 直连测试
Invoke-WebRequest -Uri "https://devai-litellm.houtai.io/health" -TimeoutSec 10
# → 超时

# 通过本地代理测试
Invoke-WebRequest -Uri "https://devai-litellm.houtai.io/health" -Proxy "http://127.0.0.1:7890"
# → 超时
```

> 无论直连还是走本地代理，HTTPS 请求均超时，确认网络层面完全不可达。

### 根因分析

```mermaid
graph TB
    subgraph "DNS 解析链路"
        D1["devai-litellm.houtai.io"] -->|CNAME| D2["vpce-xxx<br/>.ap-southeast-1<br/>.vpce.amazonaws.com"]
        D2 -->|A 记录| D3["10.102.255.29<br/>10.102.255.43<br/>10.102.255.7"]
    end

    subgraph "网络可达性"
        D3 -->|"❌ 不可达"| N1["公网电脑<br/>无法路由到 10.x.x.x"]
        D3 -->|"✅ 可达"| N2["AWS VPC 内网<br/>或 VPN 已连接"]
    end

    N1 --> R["请求超时<br/>Claude Code 报错"]
    N2 --> S["正常访问<br/>Claude Code 可用"]

    style D3 fill:#E74C3C,stroke:#333,color:#fff
    style N1 fill:#E74C3C,stroke:#333,color:#fff
    style N2 fill:#27AE60,stroke:#333,color:#fff
    style R fill:#E74C3C,stroke:#333,color:#fff
    style S fill:#27AE60,stroke:#333,color:#fff
```

**根本原因：** LiteLLM 代理部署在 AWS VPC 内网，通过 VPC Endpoint 暴露服务。域名解析到的 `10.x.x.x` 是 RFC 1918 私有地址，只有处于同一 VPC 或通过 VPN 接入该内网的机器才能访问。本地公网电脑无法路由到这些地址，导致所有请求超时。

```mermaid
graph LR
    A["本地电脑<br/>公网 IP"] -->|"❌ 路由不可达<br/>10.x.x.x 是内网地址"| B["LiteLLM 代理<br/>AWS VPC 内网<br/>10.102.255.x"]

    C["公司办公网络<br/>已通 VPN"] -->|"✅ VPN 隧道<br/>可达内网"| B

    D["AWS 同 VPC<br/>EC2 / ECS"] -->|"✅ 内网直通"| B

    style A fill:#E74C3C,stroke:#333,color:#fff
    style B fill:#D4A843,stroke:#333,color:#fff
    style C fill:#27AE60,stroke:#333,color:#fff
    style D fill:#27AE60,stroke:#333,color:#fff
```

### 解决方案

#### 方案 1：连接公司 VPN（推荐）

如果组织提供了 VPN 接入，连上后本地电脑即可路由到 AWS VPC 内网地址，代理恢复可用。

#### 方案 2：切换为直连 Anthropic 官方 API

修改 `~/.claude/settings.json`，去掉代理配置，改用官方 API：

```json
{
    "env": {
        "ANTHROPIC_MODEL": "claude-opus-4-6-20250918",
        "ANTHROPIC_BASE_URL": ""
    }
}
```

然后通过 `claude login` 登录 Anthropic 官方账号，或配置个人 API Key。

> **注意：** 直连模式需要自行承担 API 费用，且可能受到地域限制（参考 [Cursor 区域代理指南](./cursor-region-proxy-guide.md)）。

---

## 常见问题

### Q: 通过代理调用，模型会变笨吗？

**不会。** LiteLLM 只做请求转发，不会修改 prompt 或模型输出。最终调用的模型实例和直连完全相同。

### Q: 代理会看到我的对话内容吗？

**理论上可以。** 请求经过代理服务器，代理管理员有能力记录请求内容。在企业场景下这通常是合规要求（审计日志），但要确保代理服务器由可信方运营。

### Q: 如何切换回直连模式？

修改 `~/.claude/settings.json`，移除代理相关配置：

```json
{
    "env": {
        "ANTHROPIC_BASE_URL": "",
        "ANTHROPIC_MODEL": "claude-opus-4-6-20250918"
    }
}
```

然后使用 Anthropic 官方 API Key 或通过 `claude login` 登录官方账号。

### Q: 模型名为什么有 `global.anthropic.` 前缀？

这是 LiteLLM 的路由规则。LiteLLM 根据前缀决定将请求发往哪个后端提供商。直连模式下不需要此前缀，直接使用 `claude-opus-4-6-20250918` 即可。

---

## 参考资料

- [LiteLLM 官方文档](https://docs.litellm.ai/)
- [Claude Code 官方文档](https://docs.anthropic.com/en/docs/claude-code)
- [Anthropic API 参考](https://docs.anthropic.com/en/api)
