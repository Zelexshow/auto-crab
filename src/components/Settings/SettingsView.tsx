import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as dialogOpen } from "@tauri-apps/plugin-dialog";
import { Save, Key, Shield, Bot, Globe, Terminal, ChevronRight, Loader2, FolderOpen } from "lucide-react";

const PROVIDERS = [
  { value: "", label: "未配置" },
  { value: "dashscope", label: "通义千问 (DashScope)" },
  { value: "dashscope_vl", label: "通义千问 VL (视觉多模态)" },
  { value: "deepseek", label: "DeepSeek" },
  { value: "zhipu", label: "智谱 GLM" },
  { value: "moonshot", label: "月之暗面 Kimi" },
  { value: "openai", label: "OpenAI" },
  { value: "anthropic", label: "Anthropic Claude" },
  { value: "ollama", label: "Ollama (本地)" },
];

const PROVIDER_KEY_MAP: Record<string, string> = {
  dashscope: "dashscope",
  dashscope_vl: "dashscope",
  deepseek: "deepseek",
  zhipu: "zhipu",
  moonshot: "moonshot",
  openai: "openai",
  anthropic: "anthropic",
  ollama: "",
};

const MODEL_SUGGESTIONS: Record<string, { value: string; label: string }[]> = {
  dashscope: [
    { value: "qwen-max", label: "qwen-max (最强)" },
    { value: "qwen-plus", label: "qwen-plus (均衡)" },
    { value: "qwen-turbo", label: "qwen-turbo (快速)" },
    { value: "qwen3-plus", label: "qwen3-plus (最新)" },
  ],
  dashscope_vl: [
    { value: "qwen3-vl-plus", label: "qwen3-vl-plus (视觉推荐)" },
    { value: "qwen2.5-vl-plus", label: "qwen2.5-vl-plus" },
  ],
  deepseek: [
    { value: "deepseek-chat", label: "deepseek-chat (对话)" },
    { value: "deepseek-reasoner", label: "deepseek-reasoner (推理)" },
  ],
  zhipu: [
    { value: "glm-4-plus", label: "glm-4-plus" },
    { value: "glm-4", label: "glm-4" },
  ],
  moonshot: [
    { value: "moonshot-v1-128k", label: "moonshot-v1-128k (长文)" },
    { value: "moonshot-v1-32k", label: "moonshot-v1-32k" },
  ],
  openai: [
    { value: "gpt-4o", label: "gpt-4o" },
    { value: "gpt-4o-mini", label: "gpt-4o-mini" },
    { value: "o3-mini", label: "o3-mini (推理)" },
  ],
  anthropic: [
    { value: "claude-sonnet-4-20250514", label: "Claude Sonnet 4" },
    { value: "claude-3-5-haiku-20241022", label: "Claude 3.5 Haiku" },
  ],
  ollama: [
    { value: "qwen2.5:14b", label: "qwen2.5:14b" },
    { value: "llama3.1:8b", label: "llama3.1:8b" },
    { value: "deepseek-r1:14b", label: "deepseek-r1:14b" },
  ],
};

export function SettingsView() {
  const [activeTab, setActiveTab] = useState<string>("models");
  const [configLoaded, setConfigLoaded] = useState(false);

  const [primaryProvider, setPrimaryProvider] = useState("");
  const [primaryModel, setPrimaryModel] = useState("");
  const [fallbackProvider, setFallbackProvider] = useState("");
  const [fallbackModel, setFallbackModel] = useState("");
  const [fallbackEndpoint, setFallbackEndpoint] = useState("http://localhost:11434");
  const [codingProvider, setCodingProvider] = useState("");
  const [codingModel, setCodingModel] = useState("");
  const [visionProvider, setVisionProvider] = useState("");
  const [visionModel, setVisionModel] = useState("");

  const [shellEnabled, setShellEnabled] = useState(true);
  const [shellCommands, setShellCommands] = useState("git, npm, pnpm, python, cargo, node");
  const [networkEnabled, setNetworkEnabled] = useState(true);
  const [networkDomains, setNetworkDomains] = useState("");
  const [fileAccess, setFileAccess] = useState("");

  const [remoteEnabled, setRemoteEnabled] = useState(false);
  const [feishuAppId, setFeishuAppId] = useState("");
  const [feishuPollInterval, setFeishuPollInterval] = useState("30");
  const [feishuAllowedUsers, setFeishuAllowedUsers] = useState("");
  const [wechatCorpId, setWechatCorpId] = useState("");
  const [wechatAgentId, setWechatAgentId] = useState("");
  const [wechatPollInterval, setWechatPollInterval] = useState("30");

  const [autoLockMin, setAutoLockMin] = useState("15");

  const [keyName, setKeyName] = useState("");
  const [apiKeyInput, setApiKeyInput] = useState("");
  const [saving, setSaving] = useState(false);
  const [saveMsg, setSaveMsg] = useState("");
  const [configSaving, setConfigSaving] = useState(false);
  const [configSaveMsg, setConfigSaveMsg] = useState("");
  const [credentialStatuses, setCredentialStatuses] = useState<Record<string, boolean>>({});

  // Load actual config on mount
  useEffect(() => {
    invoke<{ success: boolean; data?: any }>("get_config").then((res) => {
      if (!res.success || !res.data) return;
      const cfg = res.data;
      if (cfg.models?.primary) {
        setPrimaryProvider(cfg.models.primary.provider ?? "");
        setPrimaryModel(cfg.models.primary.model ?? "");
      }
      if (cfg.models?.fallback) {
        setFallbackProvider(cfg.models.fallback.provider ?? "");
        setFallbackModel(cfg.models.fallback.model ?? "");
        setFallbackEndpoint(cfg.models.fallback.endpoint ?? "http://localhost:11434");
      }
      if (cfg.models?.coding) {
        setCodingProvider(cfg.models.coding.provider ?? "");
        setCodingModel(cfg.models.coding.model ?? "");
      }
      if (cfg.models?.vision) {
        setVisionProvider(cfg.models.vision.provider ?? "");
        setVisionModel(cfg.models.vision.model ?? "");
      }
      if (cfg.tools) {
        setShellEnabled(cfg.tools.shell_enabled ?? true);
        setShellCommands((cfg.tools.shell_allowed_commands ?? []).join(", "));
        setNetworkEnabled(cfg.tools.network_access ?? true);
        setNetworkDomains((cfg.tools.network_allowed_domains ?? []).join(", "));
        setFileAccess((cfg.tools.file_access ?? []).join(", "));
      }
      if (cfg.remote) {
        setRemoteEnabled(cfg.remote.enabled ?? false);
        if (cfg.remote.feishu) {
          setFeishuAppId(cfg.remote.feishu.app_id ?? "");
          setFeishuPollInterval(String(cfg.remote.feishu.poll_interval_secs ?? 30));
          setFeishuAllowedUsers((cfg.remote.feishu.allowed_user_ids ?? []).join(", "));
        }
        if (cfg.remote.wechat_work) {
          setWechatCorpId(cfg.remote.wechat_work.corp_id ?? "");
          setWechatAgentId(cfg.remote.wechat_work.agent_id ?? "");
          setWechatPollInterval(String(cfg.remote.wechat_work.poll_interval_secs ?? 30));
        }
      }
      if (cfg.security) {
        setAutoLockMin(String(cfg.security.auto_lock_minutes ?? 15));
      }
      setConfigLoaded(true);
    }).catch(() => setConfigLoaded(true));

    const knownKeys = ["dashscope", "deepseek", "zhipu", "moonshot", "openai", "anthropic", "feishu-secret", "wechat-work-secret"];
    invoke<{ success: boolean; data?: { key: string; exists: boolean }[] }>("check_credentials", { keys: knownKeys }).then((res) => {
      if (res.success && res.data) {
        const m: Record<string, boolean> = {};
        res.data.forEach((s) => { m[s.key] = s.exists; });
        setCredentialStatuses(m);
      }
    }).catch(() => {});
  }, []);

  const handleSaveConfig = async () => {
    setConfigSaving(true);
    setConfigSaveMsg("");
    try {
      const existing = await invoke<{ success: boolean; data?: any }>("get_config");
      const base = existing.success && existing.data ? existing.data : {};

      const cfg: any = {
        ...base,
        general: { ...(base.general || {}), language: "zh-CN", theme: base.general?.theme ?? "system", first_run: false },
        models: {
          ...(base.models || {}),
          routing: base.models?.routing ?? {},
          primary: primaryProvider ? { provider: primaryProvider, model: primaryModel, api_key_ref: `keychain://${PROVIDER_KEY_MAP[primaryProvider] || primaryProvider}` } : base.models?.primary ?? null,
          fallback: fallbackProvider ? { provider: fallbackProvider, model: fallbackModel, ...(fallbackProvider === "ollama" ? { endpoint: fallbackEndpoint } : {}), api_key_ref: fallbackProvider !== "ollama" ? `keychain://${fallbackProvider}` : undefined } : base.models?.fallback ?? null,
          coding: codingProvider ? { provider: codingProvider, model: codingModel, api_key_ref: `keychain://${PROVIDER_KEY_MAP[codingProvider] || codingProvider}` } : base.models?.coding ?? null,
          vision: visionProvider ? { provider: visionProvider, model: visionModel, api_key_ref: `keychain://${PROVIDER_KEY_MAP[visionProvider] || visionProvider}` } : base.models?.vision ?? null,
        },
        agent: base.agent ?? { name: "小蟹", personality: "professional", max_context_tokens: 128000, system_prompt: "" },
        security: { ...(base.security || {}), auto_lock_minutes: parseInt(autoLockMin) || 15 },
        tools: {
          ...(base.tools || {}),
          shell_enabled: shellEnabled,
          shell_allowed_commands: shellCommands.split(",").map((s: string) => s.trim()).filter(Boolean),
          network_access: networkEnabled,
          network_allowed_domains: networkDomains.split(",").map((s: string) => s.trim()).filter(Boolean),
          file_access: fileAccess.split(",").map((s: string) => s.trim()).filter(Boolean),
        },
        remote: {
          ...(base.remote || {}),
          enabled: remoteEnabled,
          feishu: remoteEnabled && feishuAppId ? {
            ...(base.remote?.feishu || {}),
            app_id: feishuAppId,
            app_secret_ref: "keychain://feishu-secret",
            poll_interval_secs: parseInt(feishuPollInterval) || 30,
            allowed_user_ids: feishuAllowedUsers.split(",").map((s: string) => s.trim()).filter(Boolean),
          } : base.remote?.feishu ?? null,
          wechat_work: remoteEnabled && wechatCorpId ? {
            ...(base.remote?.wechat_work || {}),
            corp_id: wechatCorpId,
            agent_id: wechatAgentId,
            secret_ref: "keychain://wechat-work-secret",
            poll_interval_secs: parseInt(wechatPollInterval) || 30,
          } : base.remote?.wechat_work ?? null,
        },
        scheduled_tasks: base.scheduled_tasks ?? { enabled: false, require_confirmation: true, jobs: [] },
      };
      await invoke("save_config", { configData: cfg });
      setConfigSaveMsg("✅ 配置已保存，重启应用后生效");
    } catch (e: any) {
      setConfigSaveMsg(`❌ 保存失败: ${e.toString()}`);
    }
    setConfigSaving(false);
  };

  const handleSaveKey = async () => {
    if (!keyName || !apiKeyInput) return;
    setSaving(true);
    setSaveMsg("");
    try {
      await invoke("store_credential", { key: keyName, secret: apiKeyInput });
      setSaveMsg(`✅ "${keyName}" 已保存到系统密钥链`);
      setApiKeyInput("");
      setKeyName("");
    } catch (e: any) {
      setSaveMsg(`❌ 保存失败: ${e.toString()}`);
    }
    setSaving(false);
  };

  const tabs = [
    { id: "models", label: "模型配置", icon: Bot },
    { id: "security", label: "安全设置", icon: Shield },
    { id: "tools", label: "工具权限", icon: Terminal },
    { id: "remote", label: "远程控制", icon: Globe },
    { id: "credentials", label: "密钥管理", icon: Key },
  ];

  return (
    <div className="flex h-full">
      {/* Left tabs */}
      <div
        className="w-44 border-r shrink-0 py-4 flex flex-col"
        style={{ borderColor: "var(--border)", background: "var(--bg-secondary)" }}
      >
        <h2 className="text-xs font-semibold px-4 pb-3 uppercase tracking-wider" style={{ color: "var(--text-muted)" }}>
          设置
        </h2>
        <nav className="flex-1 px-2 space-y-0.5">
          {tabs.map((tab) => {
            const Icon = tab.icon;
            const active = activeTab === tab.id;
            return (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className="flex items-center justify-between w-full rounded-md px-3 py-2 text-sm transition-colors"
                style={{
                  background: active ? "var(--accent)" : "transparent",
                  color: active ? "#fff" : "var(--text-secondary)",
                }}
              >
                <span className="flex items-center gap-2">
                  <Icon size={15} />
                  {tab.label}
                </span>
                {active && <ChevronRight size={14} />}
              </button>
            );
          })}
        </nav>
      </div>

      {/* Right content */}
      <div className="flex-1 overflow-y-auto flex flex-col">
        {!configLoaded ? (
          <div className="flex-1 flex items-center justify-center gap-2" style={{ color: "var(--text-muted)" }}>
            <Loader2 size={16} className="animate-spin" /> 加载配置中...
          </div>
        ) : (
        <div className="flex-1 overflow-y-auto p-6">
        <div className="max-w-xl">

          {/* ============ 模型配置 ============ */}
          {activeTab === "models" && (
            <Section title="模型配置" desc="配置 AI 模型。支持国产模型、国际模型和本地 Ollama。">
              <Card title="主模型" desc="日常对话使用">
                <Row label="提供商">
                  <Select value={primaryProvider} onChange={(v) => { setPrimaryProvider(v); setPrimaryModel(MODEL_SUGGESTIONS[v]?.[0]?.value ?? ""); }} options={PROVIDERS} />
                </Row>
                <Row label="模型名">
                  {MODEL_SUGGESTIONS[primaryProvider] ? (
                    <Select value={primaryModel} onChange={setPrimaryModel} options={[...MODEL_SUGGESTIONS[primaryProvider], { value: "__custom__", label: "自定义..." }]} />
                  ) : (
                    <Input value={primaryModel} onChange={setPrimaryModel} placeholder="输入模型名称" />
                  )}
                  {primaryModel === "__custom__" && <Input value="" onChange={setPrimaryModel} placeholder="输入自定义模型名" />}
                </Row>
                <Row label="API Key">
                  <KeyStatus provider={primaryProvider} statuses={credentialStatuses} />
                </Row>
              </Card>

              <Card title="回退模型" desc="主模型不可用时自动切换，推荐本地 Ollama">
                <Row label="提供商">
                  <Select value={fallbackProvider} onChange={setFallbackProvider} options={PROVIDERS} />
                </Row>
                <Row label="模型名">
                  <Input value={fallbackModel} onChange={setFallbackModel} placeholder="qwen2.5:14b" />
                </Row>
                {fallbackProvider === "ollama" && (
                  <Row label="端点地址">
                    <Input value={fallbackEndpoint} onChange={setFallbackEndpoint} placeholder="http://localhost:11434" />
                  </Row>
                )}
              </Card>

              <Card title="视觉模型" desc="截图分析、K线识别、图像理解（需要多模态模型）">
                <Row label="提供商">
                  <Select value={visionProvider} onChange={(v) => { setVisionProvider(v); setVisionModel(MODEL_SUGGESTIONS[v]?.[0]?.value ?? ""); }} options={PROVIDERS} />
                </Row>
                <Row label="模型名">
                  {MODEL_SUGGESTIONS[visionProvider] ? (
                    <Select value={visionModel} onChange={setVisionModel} options={MODEL_SUGGESTIONS[visionProvider]} />
                  ) : (
                    <Input value={visionModel} onChange={setVisionModel} placeholder="qwen3-vl-plus" />
                  )}
                </Row>
                <Row label="API Key">
                  <KeyStatus provider={visionProvider} statuses={credentialStatuses} />
                </Row>
              </Card>

              <Card title="编码模型" desc="代码生成和分析专用（可选）">
                <Row label="提供商">
                  <Select value={codingProvider} onChange={setCodingProvider} options={PROVIDERS} />
                </Row>
                <Row label="模型名">
                  <Input value={codingModel} onChange={setCodingModel} placeholder="deepseek-coder-v3" />
                </Row>
              </Card>
            </Section>
          )}

          {/* ============ 安全设置 ============ */}
          {activeTab === "security" && (
            <Section title="安全设置" desc="控制 Auto Crab 的安全行为。核心安全机制不可关闭。">
              <Card title="操作审批" desc="所有风险操作需要用户确认后才能执行">
                <Row label="状态">
                  <ToggleLocked on label="始终开启" />
                </Row>
                <div className="mt-2 text-xs leading-5 space-y-1" style={{ color: "var(--text-muted)" }}>
                  <p>• <b style={{ color: "var(--success)" }}>安全操作</b>（读文件、搜索）→ 自动执行</p>
                  <p>• <b style={{ color: "var(--warning)" }}>中风险</b>（写文件、Git 提交）→ 弹窗确认</p>
                  <p>• <b style={{ color: "var(--danger)" }}>高风险</b>（执行命令、删除文件）→ 密码二次验证</p>
                  <p>• <b>禁止操作</b>（格式化磁盘等）→ 永远不允许</p>
                </div>
              </Card>

              <Card title="自动锁定" desc="空闲超过设定时间后锁定，需重新验证">
                <Row label="锁定时间">
                  <div className="flex items-center gap-2">
                    <Select
                      value={autoLockMin}
                      onChange={setAutoLockMin}
                      options={[
                        { value: "5", label: "5 分钟" },
                        { value: "15", label: "15 分钟" },
                        { value: "30", label: "30 分钟" },
                        { value: "60", label: "1 小时" },
                        { value: "0", label: "不锁定" },
                      ]}
                    />
                  </div>
                </Row>
              </Card>

              <Card title="禁止操作列表" desc="以下操作永远不会被执行，无法通过任何方式绕过">
                <div className="text-xs leading-5 space-y-0.5" style={{ color: "var(--text-muted)" }}>
                  {["format_disk", "modify_boot", "disable_firewall", "access_credentials_raw", "modify_system_registry", "shutdown_system"].map((op) => (
                    <p key={op} className="flex items-center gap-1.5">
                      <span className="w-1.5 h-1.5 rounded-full shrink-0" style={{ background: "var(--danger)" }} />
                      <code className="text-[11px]">{op}</code>
                    </p>
                  ))}
                </div>
              </Card>
            </Section>
          )}

          {/* ============ 工具权限 ============ */}
          {activeTab === "tools" && (
            <Section title="工具权限" desc="控制 AI 助理可以使用的工具和访问范围。">
              <Card title="Shell 执行" desc="允许 AI 执行命令行命令">
                <Row label="启用">
                  <Toggle on={shellEnabled} onChange={setShellEnabled} />
                </Row>
                {shellEnabled && (
                  <Row label="允许的命令">
                    <Input
                      value={shellCommands}
                      onChange={setShellCommands}
                      placeholder="git, npm, python..."
                    />
                    <p className="text-[11px] mt-1" style={{ color: "var(--text-muted)" }}>
                      逗号分隔，留空允许所有命令（不推荐）
                    </p>
                  </Row>
                )}
              </Card>

              <Card title="文件访问" desc="AI 可读写的目录范围">
                <Row label="允许的目录">
                  <div className="flex gap-2">
                    <div className="flex-1">
                      <Input
                        value={fileAccess}
                        onChange={setFileAccess}
                        placeholder="留空 = 允许所有目录"
                      />
                    </div>
                    <button
                      onClick={async () => {
                        const dirs = await dialogOpen({ directory: true, multiple: true, title: "选择允许访问的目录（可多选）" });
                        if (dirs) {
                          const selected = Array.isArray(dirs) ? dirs : [dirs];
                          setFileAccess((prev) => {
                            const existing = prev ? prev.split(",").map((s: string) => s.trim()).filter(Boolean) : [];
                            const merged = [...new Set([...existing, ...selected])];
                            return merged.join(", ");
                          });
                        }
                      }}
                      className="shrink-0 px-3 py-1.5 rounded-md text-xs flex items-center gap-1"
                      style={{ background: "var(--bg-tertiary)", color: "var(--text-secondary)" }}
                    >
                      <FolderOpen size={13} /> 选择
                    </button>
                  </div>
                  <p className="text-[11px] mt-1" style={{ color: "var(--text-muted)" }}>
                    逗号分隔的目录路径。留空表示允许访问所有目录。点击"选择"可用系统文件夹对话框添加。
                  </p>
                </Row>
              </Card>

              <Card title="网络访问" desc="AI 是否可以发起外部网络请求">
                <Row label="启用">
                  <Toggle on={networkEnabled} onChange={setNetworkEnabled} />
                </Row>
                {networkEnabled && (
                  <Row label="允许的域名">
                    <Input
                      value={networkDomains}
                      onChange={setNetworkDomains}
                      placeholder="留空 = 允许所有域名"
                    />
                    <p className="text-[11px] mt-1" style={{ color: "var(--text-muted)" }}>
                      逗号分隔，例如: github.com, *.npmjs.org
                    </p>
                  </Row>
                )}
              </Card>
            </Section>
          )}

          {/* ============ 远程控制 ============ */}
          {activeTab === "remote" && (
            <Section title="远程控制" desc="通过飞书或企业微信远程控制 Auto Crab。所有远程操作同样经过安全审批。">
              <Card title="远程控制总开关" desc="">
                <Row label="启用远程控制">
                  <Toggle on={remoteEnabled} onChange={setRemoteEnabled} />
                </Row>
              </Card>

              {remoteEnabled && (
                <>
                  <Card title="飞书 Bot" desc="通过飞书发送指令控制桌面端">
                    <Row label="App ID">
                      <Input value={feishuAppId} onChange={setFeishuAppId} placeholder="cli_xxxxxxxx" />
                    </Row>
                    <Row label="App Secret">
                      <span className="text-xs" style={{ color: "var(--text-muted)" }}>
                        在「密钥管理」中保存，引用: <code className="px-1 py-0.5 rounded text-[11px]" style={{ background: "var(--bg-tertiary)" }}>keychain://feishu-secret</code>
                      </span>
                    </Row>
                    <Row label="轮询间隔">
                      <div className="flex items-center gap-2">
                        <Input value={feishuPollInterval} onChange={setFeishuPollInterval} placeholder="30" />
                        <span className="text-xs shrink-0" style={{ color: "var(--text-muted)" }}>秒</span>
                      </div>
                    </Row>
                    <Row label="允许的用户">
                      <Input value={feishuAllowedUsers} onChange={setFeishuAllowedUsers} placeholder="user_id_1, user_id_2" />
                      <p className="text-[11px] mt-1" style={{ color: "var(--text-muted)" }}>
                        逗号分隔的飞书用户 ID。留空 = 拒绝所有（安全默认）
                      </p>
                    </Row>
                  </Card>

                  <Card title="企业微信 Bot" desc="通过企业微信发送指令控制桌面端">
                    <Row label="Corp ID">
                      <Input value={wechatCorpId} onChange={setWechatCorpId} placeholder="wxxxxxxxxx" />
                    </Row>
                    <Row label="Agent ID">
                      <Input value={wechatAgentId} onChange={setWechatAgentId} placeholder="1000001" />
                    </Row>
                    <Row label="Secret">
                      <span className="text-xs" style={{ color: "var(--text-muted)" }}>
                        在「密钥管理」中保存，引用: <code className="px-1 py-0.5 rounded text-[11px]" style={{ background: "var(--bg-tertiary)" }}>keychain://wechat-work-secret</code>
                      </span>
                    </Row>
                    <Row label="轮询间隔">
                      <div className="flex items-center gap-2">
                        <Input value={wechatPollInterval} onChange={setWechatPollInterval} placeholder="30" />
                        <span className="text-xs shrink-0" style={{ color: "var(--text-muted)" }}>秒</span>
                      </div>
                    </Row>
                  </Card>
                </>
              )}
            </Section>
          )}

          {/* ============ 密钥管理 ============ */}
          {activeTab === "credentials" && (
            <Section title="密钥管理" desc="所有 API 密钥安全存储在系统密钥链中（Windows Credential Store / macOS Keychain / Linux Secret Service），不存储在配置文件中。">
              <Card title="添加 / 更新密钥" desc="将 API Key 安全存储到系统密钥链">
                <Row label="密钥名称">
                  <Select
                    value={keyName}
                    onChange={setKeyName}
                    options={[
                      { value: "", label: "选择或输入..." },
                      { value: "dashscope", label: "dashscope (通义千问)" },
                      { value: "deepseek", label: "deepseek" },
                      { value: "zhipu", label: "zhipu (智谱)" },
                      { value: "moonshot", label: "moonshot (Kimi)" },
                      { value: "openai", label: "openai" },
                      { value: "anthropic", label: "anthropic (Claude)" },
                      { value: "feishu-secret", label: "feishu-secret (飞书)" },
                      { value: "wechat-work-secret", label: "wechat-work-secret (企业微信)" },
                    ]}
                  />
                </Row>
                <Row label="API Key">
                  <input
                    type="password"
                    value={apiKeyInput}
                    onChange={(e) => setApiKeyInput(e.target.value)}
                    className="w-full rounded-md px-3 py-1.5 text-sm outline-none"
                    style={{
                      background: "var(--bg-primary)",
                      border: "1px solid var(--border)",
                      color: "var(--text-primary)",
                    }}
                    placeholder="sk-..."
                  />
                </Row>
                <div className="flex items-center gap-3 mt-1">
                  <button
                    onClick={handleSaveKey}
                    disabled={saving || !keyName || !apiKeyInput}
                    className="flex items-center gap-1.5 px-4 py-1.5 rounded-md text-sm text-white transition-colors disabled:opacity-40"
                    style={{ background: "#07c160" }}
                  >
                    <Save size={13} />
                    {saving ? "保存中..." : "保存到密钥链"}
                  </button>
                  {saveMsg && (
                    <span className="text-xs" style={{ color: saveMsg.startsWith("✅") ? "var(--success)" : "var(--danger)" }}>
                      {saveMsg}
                    </span>
                  )}
                </div>
              </Card>

              <Card title="已保存的密钥" desc="系统密钥链中当前存在的凭据">
                <div className="space-y-1.5">
                  {["dashscope", "deepseek", "zhipu", "moonshot", "openai", "anthropic", "feishu-secret", "wechat-work-secret"].map((k) => (
                    <CredentialRow key={k} name={k} exists={credentialStatuses[k]} onEdit={(name, currentPreview) => {
                      setKeyName(name);
                      setApiKeyInput("");
                      setSaveMsg(currentPreview ? `当前值: ${currentPreview}（输入新值将覆盖）` : "");
                    }} />
                  ))}
                </div>
              </Card>

              <Card title="说明" desc="">
                <div className="text-xs leading-5 space-y-1" style={{ color: "var(--text-muted)" }}>
                  <p>• 密钥存储在操作系统级别的加密存储中，不会出现在配置文件里</p>
                  <p>• 配置文件中使用 <code className="px-1 py-0.5 rounded text-[11px]" style={{ background: "var(--bg-tertiary)" }}>keychain://名称</code> 引用密钥</p>
                  <p>• 更新密钥只需使用相同的名称重新保存即可覆盖</p>
                </div>
              </Card>
            </Section>
          )}

        </div>
        </div>
        )}

        {/* Bottom save bar (shown for non-credential tabs) */}
        {configLoaded && activeTab !== "credentials" && (
          <div
            className="shrink-0 px-6 py-3 flex items-center gap-3 border-t"
            style={{ borderColor: "var(--border)", background: "var(--bg-secondary)" }}
          >
            <button
              onClick={handleSaveConfig}
              disabled={configSaving}
              className="flex items-center gap-1.5 px-4 py-1.5 rounded-md text-sm text-white transition-colors disabled:opacity-50"
              style={{ background: "var(--accent)" }}
            >
              {configSaving ? <Loader2 size={13} className="animate-spin" /> : <Save size={13} />}
              {configSaving ? "保存中..." : "保存配置"}
            </button>
            {configSaveMsg && (
              <span className="text-xs" style={{ color: configSaveMsg.startsWith("✅") ? "var(--success)" : "var(--danger)" }}>
                {configSaveMsg}
              </span>
            )}
            <span className="text-xs ml-auto" style={{ color: "var(--text-muted)" }}>
              部分配置（远程控制、工具权限）需重启应用后生效
            </span>
          </div>
        )}
      </div>
    </div>
  );
}

/* ================= KeyStatus Component ================= */

function KeyStatus({ provider, statuses }: { provider: string; statuses: Record<string, boolean> }) {
  const [preview, setPreview] = useState("");
  const keyName = PROVIDER_KEY_MAP[provider] ?? provider;

  useEffect(() => {
    if (keyName && statuses[keyName]) {
      invoke<{ success: boolean; data?: string }>("get_credential_preview", { key: keyName }).then((res) => {
        setPreview(res.data ?? "");
      }).catch(() => {});
    } else {
      setPreview("");
    }
  }, [keyName, statuses]);

  if (!provider || provider === "ollama") return <span className="text-xs" style={{ color: "var(--text-muted)" }}>本地模型，无需 API Key</span>;
  const exists = statuses[keyName];
  return (
    <div className="flex items-center gap-2">
      <span className="inline-block w-2 h-2 rounded-full" style={{ background: exists ? "var(--success)" : "var(--danger)" }} />
      <span className="text-xs" style={{ color: "var(--text-muted)" }}>
        {exists
          ? `已配置 ${preview ? `(${preview})` : ""} — keychain://${keyName}`
          : `未配置，请在「密钥管理」中保存 keychain://${keyName}`}
      </span>
    </div>
  );
}

/* ================= CredentialRow Component ================= */

function CredentialRow({ name, exists, onEdit }: { name: string; exists: boolean; onEdit: (name: string, currentPreview: string) => void }) {
  const [preview, setPreview] = useState("");
  useEffect(() => {
    if (exists) {
      invoke<{ success: boolean; data?: string }>("get_credential_preview", { key: name }).then((res) => {
        setPreview(res.data ?? "");
      }).catch(() => {});
    }
  }, [name, exists]);

  return (
    <div className="flex items-center justify-between text-xs py-1.5 px-2 rounded" style={{ background: "var(--bg-primary)" }}>
      <div className="flex items-center gap-2">
        <span className="inline-block w-2 h-2 rounded-full" style={{ background: exists ? "var(--success)" : "var(--bg-tertiary)" }} />
        <code>{name}</code>
        {exists && preview && <span style={{ color: "var(--text-muted)" }}>({preview})</span>}
      </div>
      <div className="flex items-center gap-2">
        {exists ? (
          <span style={{ color: "var(--success)" }}>已保存</span>
        ) : (
          <span style={{ color: "var(--text-muted)" }}>未配置</span>
        )}
        <button
          onClick={() => onEdit(name, preview)}
          className="px-2 py-0.5 rounded text-[11px] transition-colors"
          style={{ background: "var(--bg-tertiary)", color: "var(--text-secondary)" }}
        >
          {exists ? "修改" : "添加"}
        </button>
      </div>
    </div>
  );
}

/* ================= Reusable Components ================= */

function Section({ title, desc, children }: { title: string; desc: string; children: React.ReactNode }) {
  return (
    <div className="space-y-4">
      <div>
        <h3 className="text-base font-semibold">{title}</h3>
        {desc && <p className="text-xs mt-1" style={{ color: "var(--text-muted)" }}>{desc}</p>}
      </div>
      {children}
    </div>
  );
}

function Card({ title, desc, children }: { title: string; desc: string; children: React.ReactNode }) {
  return (
    <div
      className="rounded-lg p-4 space-y-3"
      style={{ background: "var(--bg-secondary)", border: "1px solid var(--border)" }}
    >
      {title && <h4 className="text-sm font-medium">{title}</h4>}
      {desc && <p className="text-xs -mt-2" style={{ color: "var(--text-muted)" }}>{desc}</p>}
      {children}
    </div>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-1">
      <label className="text-xs font-medium" style={{ color: "var(--text-secondary)" }}>{label}</label>
      {children}
    </div>
  );
}

function Input({ value, onChange, placeholder }: { value: string; onChange: (v: string) => void; placeholder?: string }) {
  return (
    <input
      type="text"
      value={value}
      onChange={(e) => onChange(e.target.value)}
      placeholder={placeholder}
      className="w-full rounded-md px-3 py-1.5 text-sm outline-none"
      style={{
        background: "var(--bg-primary)",
        border: "1px solid var(--border)",
        color: "var(--text-primary)",
      }}
    />
  );
}

function Select({ value, onChange, options }: {
  value: string;
  onChange: (v: string) => void;
  options: { value: string; label: string }[];
}) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      className="w-full rounded-md px-3 py-1.5 text-sm outline-none appearance-none cursor-pointer"
      style={{
        background: "var(--bg-primary)",
        border: "1px solid var(--border)",
        color: "var(--text-primary)",
        backgroundImage: `url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 24 24' fill='none' stroke='%2394a3b8' stroke-width='2'%3E%3Cpath d='m6 9 6 6 6-6'/%3E%3C/svg%3E")`,
        backgroundRepeat: "no-repeat",
        backgroundPosition: "right 10px center",
        paddingRight: "32px",
      }}
    >
      {options.map((o) => (
        <option key={o.value} value={o.value}>{o.label}</option>
      ))}
    </select>
  );
}

function Toggle({ on, onChange }: { on: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      onClick={() => onChange(!on)}
      className="w-10 h-5 rounded-full relative transition-colors"
      style={{ background: on ? "#07c160" : "var(--bg-tertiary)" }}
    >
      <div
        className="w-4 h-4 rounded-full bg-white absolute top-0.5 transition-all shadow-sm"
        style={{ left: on ? "22px" : "2px" }}
      />
    </button>
  );
}

function ToggleLocked({ on, label }: { on: boolean; label: string }) {
  return (
    <div className="flex items-center gap-2">
      <div
        className="w-10 h-5 rounded-full relative opacity-70 cursor-not-allowed"
        style={{ background: on ? "#07c160" : "var(--bg-tertiary)" }}
      >
        <div
          className="w-4 h-4 rounded-full bg-white absolute top-0.5 shadow-sm"
          style={{ left: on ? "22px" : "2px" }}
        />
      </div>
      <span className="text-xs" style={{ color: "var(--text-muted)" }}>{label}</span>
    </div>
  );
}
