# 飞书远程控制配置指南

通过飞书 Bot 从手机远程控制桌面端 Auto Crab AI 助理。

---

## 架构原理

```
手机飞书 App → 飞书服务器 → Webhook → Auto Crab 桌面端
                                ↓
                        (本地 127.0.0.1:18790)
                                ↓
                      安全审批 → Agent 执行 → 回复到飞书
```

Auto Crab 在本地启动一个 webhook 服务器（端口 18790），通过隧道工具暴露给飞书服务器。
所有远程操作同样经过安全审批机制。

---

## 第一步：创建飞书应用

1. 打开 [飞书开放平台](https://open.feishu.cn/)，登录你的飞书账号

2. 点击 **"创建企业自建应用"**
   - 应用名称：`Auto Crab`
   - 应用描述：`桌面 AI 助理远程控制`

3. 进入应用详情页，记录以下信息：
   - **App ID**（如 `cli_a5xxxxx`）
   - **App Secret**（点击查看并复制）

## 第二步：配置应用权限

在应用详情页 → **权限管理** → 搜索并开通以下权限：

| 权限 | 说明 |
|------|------|
| `im:message:send_as_bot` | 以机器人身份发消息 |
| `im:message` | 获取与发送消息 |
| `im:message.receive_v1` | 接收消息事件 |

## 第三步：配置事件订阅

1. 应用详情页 → **事件订阅**
2. 请求地址配置：填写你的 webhook 地址（见第五步）
3. 添加事件：
   - `im.message.receive_v1`（接收消息）

## 第四步：在 Auto Crab 中配置

### 4.1 保存飞书密钥

在 Auto Crab 应用中：

1. **设置 → 密钥管理**
2. 密钥名称选择 `feishu-secret`
3. 粘贴你的 **App Secret**
4. 点击"保存到密钥链"

### 4.2 编辑配置文件

打开配置文件（`%APPDATA%\com.zelex.auto-crab\auto-crab.toml`），修改远程控制部分：

```toml
[remote]
enabled = true

[remote.feishu]
app_id = "cli_a5xxxxx"                    # 替换为你的 App ID
app_secret_ref = "keychain://feishu-secret"
poll_interval_secs = 30
allowed_user_ids = ["ou_xxxxx"]           # 替换为你的飞书 Open ID
```

### 如何获取你的飞书 Open ID

1. 在飞书开放平台 → **API 调试台**
2. 调用 `GET /open-apis/authen/v1/user_info`
3. 返回结果中的 `open_id` 就是你的 ID

或者：先不填 `allowed_user_ids`（留空），发一条消息给 Bot，查看 Auto Crab 终端日志中打印的 user_id，然后填入。

> ⚠️ **安全提醒**：`allowed_user_ids` 留空时会拒绝所有消息（安全默认）。务必填入你自己的 ID。

## 第五步：设置隧道（暴露本地 webhook）

Auto Crab 的 webhook 服务器运行在 `127.0.0.1:18790`，飞书服务器需要一个公网地址来推送事件。

### 方案 A：使用 ngrok（推荐，最简单）

```bash
# 安装 ngrok（https://ngrok.com/）
# 注册免费账号，获取 authtoken

ngrok http 18790
```

ngrok 会给你一个公网地址，如：`https://abc123.ngrok-free.app`

把这个地址填到飞书应用的 **事件订阅 → 请求地址** 中：
```
https://abc123.ngrok-free.app
```

### 方案 B：使用 Cloudflare Tunnel（更稳定）

```bash
# 安装 cloudflared
cloudflared tunnel --url http://127.0.0.1:18790
```

### 方案 C：使用 cpolar（国内友好）

```bash
# 安装 cpolar（https://www.cpolar.com/）
cpolar http 18790
```

## 第六步：发布应用并测试

1. 在飞书开放平台，点击 **版本管理 → 创建版本 → 申请发布**
2. 发布后，在飞书中搜索你的 Bot 名称，开始对话

### 测试指令

| 发送内容 | 效果 |
|---------|------|
| `你好` | AI 对话，Bot 回复 |
| `/status` | 查询 Auto Crab 状态 |
| `/task 帮我整理桌面文件` | 创建远程任务 |
| `/approve abc123` | 批准操作（ID 从推送消息中获取） |
| `/reject abc123` | 拒绝操作 |

---

## 安全机制

- **用户白名单**：只有 `allowed_user_ids` 中的用户才能控制，其他人的消息会被拒绝并记录日志
- **操作审批**：远程执行的危险操作同样会触发审批流程，审批请求会推送到飞书，你可以在手机上直接 `/approve` 或 `/reject`
- **审计日志**：所有远程操作都记录在审计日志中，标记来源为 `feishu`
- **无公网端口**：Auto Crab 本身不监听公网端口，只通过隧道暴露 webhook 给飞书

---

## 常见问题

**Q: ngrok 每次重启地址会变？**
A: 免费版确实会变。可以使用 ngrok 付费版固定域名，或换用 Cloudflare Tunnel 绑定自己的域名。

**Q: 飞书提示"请求地址校验失败"？**
A: 确保 ngrok 已启动且地址填写正确。Auto Crab 会自动响应飞书的 challenge 验证。

**Q: Bot 收到消息但没反应？**
A: 检查 Auto Crab 终端日志。如果显示 "Rejected message from unauthorized user"，说明 `allowed_user_ids` 没有配置正确。
