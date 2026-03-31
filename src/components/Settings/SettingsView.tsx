import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as dialogOpen } from "@tauri-apps/plugin-dialog";
import { Save, Key, Shield, Bot, Globe, Terminal, ChevronRight, Loader2, FolderOpen, Clock, Sparkles, Plus, Trash2, Upload, BookOpen } from "lucide-react";

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

  const [searchProvider, setSearchProvider] = useState("auto");
  const [serpapiApiKey, setSerpapiApiKey] = useState("");
  const [braveApiKey, setBraveApiKey] = useState("");
  const [tavilyApiKey, setTavilyApiKey] = useState("");
  const [searchStats, setSearchStats] = useState<any>(null);

  const [knowledgeEnabled, setKnowledgeEnabled] = useState(false);
  const [vaultPath, setVaultPath] = useState("");
  const [vaultSaveConversations, setVaultSaveConversations] = useState(false);
  const [vaultRouting, setVaultRouting] = useState<Record<string, string>>({
    invest: "invest-explore",
    boss: "boss-explore",
    news: "hot-news",
    default: "general",
  });

  const [remoteEnabled, setRemoteEnabled] = useState(false);
  const [feishuAppId, setFeishuAppId] = useState("");
  const [feishuPollInterval, setFeishuPollInterval] = useState("30");
  const [feishuAllowedUsers, setFeishuAllowedUsers] = useState("");
  const [wechatCorpId, setWechatCorpId] = useState("");
  const [wechatAgentId, setWechatAgentId] = useState("");
  const [wechatPollInterval, setWechatPollInterval] = useState("30");

  const [autoLockMin, setAutoLockMin] = useState("15");

  // Scheduled tasks
  const [schedEnabled, setSchedEnabled] = useState(false);
  const [schedJobs, setSchedJobs] = useState<{ name: string; cron: string; action: string; auto_execute: boolean; skill_ref?: string }[]>([]);
  const [expandedJob, setExpandedJob] = useState<number | null>(null);
  const [newJobName, setNewJobName] = useState("");
  const [newJobCron, setNewJobCron] = useState("");
  const [newJobAction, setNewJobAction] = useState("");
  const [newJobSkillRef, setNewJobSkillRef] = useState("");

  // User skills (named, stored as individual .md files)
  const [userSkills, setUserSkills] = useState<{ name: string; content: string; keywords?: string[]; always_on?: boolean }[]>([]);
  const [newSkillName, setNewSkillName] = useState("");
  const [newSkillKeywords, setNewSkillKeywords] = useState("");
  const [newSkillAlwaysOn, setNewSkillAlwaysOn] = useState(false);
  const [newSkillContent, setNewSkillContent] = useState("");
  const [expandedSkill, setExpandedSkill] = useState<number | null>(null);
  const [customInstructions, setCustomInstructions] = useState("");
  const [skillsDir, setSkillsDir] = useState("");
  const [dragOver, setDragOver] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const handleSkillFiles = async (files: FileList | null) => {
    if (!files) return;
    const imported: { name: string; content: string }[] = [];
    for (let i = 0; i < files.length; i++) {
      const file = files[i];
      if (!file.name.endsWith(".md") && !file.name.endsWith(".txt")) continue;
      const content = await file.text();
      const name = file.name.replace(/\.(md|txt)$/, "");
      if (userSkills.some(s => s.name === name)) continue;
      const skill = { name, content };
      await invoke("save_skill", { skill }).catch(() => {});
      imported.push(skill);
    }
    if (imported.length > 0) {
      setUserSkills(prev => [...prev, ...imported]);
    }
  };

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
      if (cfg.search) {
        setSearchProvider(cfg.search.provider ?? "auto");
        setSerpapiApiKey(cfg.search.serpapi_api_key ?? "");
        setBraveApiKey(cfg.search.brave_api_key ?? "");
        setTavilyApiKey(cfg.search.tavily_api_key ?? "");
      }
      if (cfg.knowledge) {
        setKnowledgeEnabled(cfg.knowledge.enabled ?? false);
        setVaultPath(cfg.knowledge.vault_path ?? "");
        setVaultSaveConversations(cfg.knowledge.save_conversations ?? false);
        if (cfg.knowledge.routing) setVaultRouting(cfg.knowledge.routing);
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
      if (cfg.scheduled_tasks) {
        setSchedEnabled(cfg.scheduled_tasks.enabled ?? false);
        setSchedJobs(cfg.scheduled_tasks.jobs ?? []);
      }
      if (cfg.agent) {
        setCustomInstructions(cfg.agent.custom_instructions ?? "");
      }
      setConfigLoaded(true);
    }).catch(() => setConfigLoaded(true));

    invoke<{ success: boolean; data?: { name: string; content: string }[] }>("list_skills").then((res) => {
      if (res.success && res.data) setUserSkills(res.data);
    }).catch(() => {});

    invoke<{ success: boolean; data?: string }>("get_skills_dir").then((res) => {
      if (res.success && res.data) setSkillsDir(res.data);
    }).catch(() => {});

    invoke<{ success: boolean; data?: any }>("get_search_usage_stats").then((res) => {
      if (res.success && res.data) setSearchStats(res.data);
    }).catch(() => {});

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
        agent: {
          ...(base.agent || {}),
          name: base.agent?.name ?? "小蟹",
          personality: base.agent?.personality ?? "professional",
          max_context_tokens: base.agent?.max_context_tokens ?? 128000,
          system_prompt: base.agent?.system_prompt ?? "",
          custom_instructions: customInstructions,
        },
        scheduled_tasks: {
          enabled: schedEnabled,
          require_confirmation: false,
          jobs: schedJobs,
        },
        search: {
          provider: searchProvider,
          serpapi_api_key: serpapiApiKey,
          brave_api_key: braveApiKey,
          tavily_api_key: tavilyApiKey,
        },
        knowledge: {
          enabled: knowledgeEnabled,
          vault_path: vaultPath,
          save_conversations: vaultSaveConversations,
          routing: vaultRouting,
        },
      };
      await invoke("save_config", { configData: cfg });

      // Save skills to individual .md files
      for (const skill of userSkills) {
        await invoke("save_skill", { skill });
      }

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
    { id: "knowledge", label: "知识库", icon: BookOpen },
    { id: "schedule", label: "定时任务", icon: Clock },
    { id: "skills", label: "自定义技能", icon: Sparkles },
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

              <Card title="搜索引擎 API" desc="配置搜索 API 获取更可靠的全球搜索结果。未配置时使用免费的 DuckDuckGo/SearXNG 爬虫。">
                <Row label="搜索方式">
                  <select
                    className="px-2 py-1 rounded text-sm"
                    style={{ background: "var(--bg-secondary)", color: "var(--text-primary)", border: "1px solid var(--border-primary)" }}
                    value={searchProvider}
                    onChange={(e) => setSearchProvider(e.target.value)}
                  >
                    <option value="auto">自动（优先 API，回退爬虫）</option>
                    <option value="serpapi">SerpApi（Google 搜索）</option>
                    <option value="brave">Brave Search</option>
                    <option value="tavily">Tavily（AI agent 专用）</option>
                  </select>
                </Row>
                <Row label="SerpApi Key">
                  <Input
                    value={serpapiApiKey}
                    onChange={setSerpapiApiKey}
                    placeholder="xxxxxxxx（免费 250 次/月）"
                    type="password"
                  />
                  <p className="text-[11px] mt-1" style={{ color: "var(--text-muted)" }}>
                    免费注册：<a href="https://serpapi.com" target="_blank" rel="noopener noreferrer" style={{ color: "var(--accent)" }}>serpapi.com</a>（无需信用卡，250 次/月）
                  </p>
                </Row>
                <Row label="Brave API Key">
                  <Input
                    value={braveApiKey}
                    onChange={setBraveApiKey}
                    placeholder="BSAxxxxxxxx（免费 1000 次/月）"
                    type="password"
                  />
                  <p className="text-[11px] mt-1" style={{ color: "var(--text-muted)" }}>
                    免费注册：<a href="https://brave.com/search/api/" target="_blank" rel="noopener noreferrer" style={{ color: "var(--accent)" }}>brave.com/search/api</a>（1000 次/月）
                  </p>
                </Row>
                <Row label="Tavily API Key">
                  <Input
                    value={tavilyApiKey}
                    onChange={setTavilyApiKey}
                    placeholder="tvly-xxxxxxxx（免费 1000 credits/月）"
                    type="password"
                  />
                  <p className="text-[11px] mt-1" style={{ color: "var(--text-muted)" }}>
                    免费注册：<a href="https://tavily.com" target="_blank" rel="noopener noreferrer" style={{ color: "var(--accent)" }}>tavily.com</a>（1000 credits/月，AI agent 优化搜索）
                  </p>
                </Row>
                <div className="mt-2 p-2 rounded text-[11px]" style={{ background: "var(--bg-tertiary)", color: "var(--text-muted)" }}>
                  搜索优先级：{searchProvider === "tavily" ? "Tavily" : searchProvider === "serpapi" ? "SerpApi (Google)" : searchProvider === "brave" ? "Brave" : tavilyApiKey || serpapiApiKey || braveApiKey ? "API 优先" : "DuckDuckGo"} → {tavilyApiKey && searchProvider !== "serpapi" && searchProvider !== "brave" ? "Tavily → " : ""}{serpapiApiKey && searchProvider !== "tavily" && searchProvider !== "brave" ? "SerpApi → " : ""}{braveApiKey && searchProvider !== "tavily" && searchProvider !== "serpapi" ? "Brave → " : ""}DuckDuckGo → SearXNG
                </div>
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

          {/* ============ 知识库 ============ */}
          {activeTab === "knowledge" && (
            <Section title="知识库集成" desc="将日报、分析报告、对话产出自动保存到 Obsidian 知识库，按类型智能分类，实现持久化积累与周度回顾。">
              <Card title="知识库配置" desc="连接你的 Obsidian Vault，报告将自动按类型归档">
                <Row label="启用知识库同步">
                  <Toggle on={knowledgeEnabled} onChange={setKnowledgeEnabled} />
                </Row>
                <Row label="知识库根目录">
                  <div className="flex gap-2">
                    <Input value={vaultPath} onChange={setVaultPath} placeholder="C:/Workspace/AI-Assistant/self-rag" />
                    <button
                      onClick={async () => {
                        const dir = await dialogOpen({ directory: true, title: "选择知识库根目录" });
                        if (dir) setVaultPath(dir as string);
                      }}
                      className="px-3 py-1.5 rounded-md text-xs whitespace-nowrap"
                      style={{ background: "var(--bg-tertiary)", color: "var(--text-secondary)", border: "1px solid var(--border)" }}
                    >
                      <FolderOpen size={14} />
                    </button>
                  </div>
                </Row>
                <Row label="保存对话内容">
                  <div className="flex items-center gap-2">
                    <Toggle on={vaultSaveConversations} onChange={setVaultSaveConversations} />
                    <span className="text-[11px]" style={{ color: "var(--text-muted)" }}>主动对话中的重要产出也存入知识库</span>
                  </div>
                </Row>
              </Card>

              <Card title="智能分类路由" desc="不同类型的内容自动归档到对应目录">
                <div className="space-y-2">
                  {[
                    { key: "invest", icon: "📈", label: "投资/理财", desc: "投资简报、盘面分析、行情日报" },
                    { key: "boss", icon: "💡", label: "创业/副业", desc: "创业灵感、商业机会分析" },
                    { key: "news", icon: "📰", label: "科技/热点", desc: "科技日报、选题推荐、热点新闻" },
                    { key: "default", icon: "📁", label: "其他/通用", desc: "未匹配到上述类别的内容" },
                  ].map(item => (
                    <div key={item.key} className="flex items-center gap-3 p-2 rounded" style={{ background: "var(--bg-tertiary)" }}>
                      <span className="text-lg">{item.icon}</span>
                      <div className="flex-1 min-w-0">
                        <div className="text-xs font-medium" style={{ color: "var(--text-primary)" }}>{item.label}</div>
                        <div className="text-[10px]" style={{ color: "var(--text-muted)" }}>{item.desc}</div>
                      </div>
                      <span className="text-[11px] font-mono px-2 py-0.5 rounded" style={{ background: "var(--bg-primary)", color: "var(--accent)" }}>
                        {vaultRouting[item.key] || (item.key === "invest" ? "invest-explore" : item.key === "boss" ? "boss-explore" : item.key === "news" ? "hot-news" : "general")}
                      </span>
                    </div>
                  ))}
                </div>
              </Card>

              <div className="text-xs leading-5 space-y-1 p-3 rounded-lg" style={{ background: "var(--bg-secondary)", border: "1px solid var(--border)", color: "var(--text-muted)" }}>
                <p className="font-medium" style={{ color: "var(--text-secondary)" }}>工作方式</p>
                <p>• 每次定时任务产出后，根据任务类型自动存入对应目录</p>
                <p>• 文件结构：<code className="px-1 py-0.5 rounded text-[11px]" style={{ background: "var(--bg-tertiary)" }}>invest-explore/2026-03-28/0935-晨间投资简报.md</code></p>
                <p>• 每个文件带 YAML frontmatter（date/tags/category），Obsidian 可直接检索和关联</p>
                <p>• 周度复盘自动读取三个目录的近 7 天笔记，生成综合分析报告</p>
                <p>• 支持 Obsidian、Logseq 或任何基于 Markdown 文件的知识库工具</p>
              </div>
            </Section>
          )}

          {/* ============ 定时任务 ============ */}
          {activeTab === "schedule" && (
            <Section title="定时任务" desc="配置定时投资报告、科技日报等自动推送任务。通过飞书/企业微信定时接收分析报告。">
              <Card title="总开关" desc="">
                <div className="flex items-center justify-between">
                  <span className="text-sm">启用定时任务</span>
                  <Toggle on={schedEnabled} onChange={setSchedEnabled} />
                </div>
                {schedEnabled && schedJobs.length === 0 && (
                  <div className="mt-2 p-3 rounded text-xs" style={{ background: "var(--bg-primary)", color: "var(--text-muted)" }}>
                    暂无任务，点击下方「添加」或「加载默认」来创建。需先配置远程控制（飞书）才能接收推送。
                  </div>
                )}
              </Card>

              {schedEnabled && (
                <>
                  {schedJobs.map((job, idx) => {
                    const isExpanded = expandedJob === idx;
                    return (
                    <div key={idx} className="rounded-lg" style={{ background: "var(--bg-secondary)", border: "1px solid var(--border)" }}>
                      <button
                        className="w-full flex items-center justify-between p-4 text-left"
                        onClick={() => setExpandedJob(isExpanded ? null : idx)}
                      >
                        <div className="flex-1 min-w-0">
                          <div className="flex items-center gap-2">
                            <h4 className="text-sm font-medium">{job.name}</h4>
                            {job.skill_ref && (
                              <span className="px-1.5 py-0.5 rounded text-[10px]" style={{ background: "var(--accent)", color: "white", opacity: 0.8 }}>
                                {job.skill_ref}
                              </span>
                            )}
                          </div>
                          <p className="text-[11px] mt-0.5" style={{ color: "var(--text-muted)" }}>Cron: {job.cron}</p>
                          {!isExpanded && (
                            <p className="text-xs mt-1 truncate" style={{ color: "var(--text-secondary)" }}>
                              {job.action.slice(0, 80)}{job.action.length > 80 ? "..." : ""}
                            </p>
                          )}
                        </div>
                        <ChevronRight size={14} className="shrink-0 ml-2 transition-transform" style={{ transform: isExpanded ? "rotate(90deg)" : "none", color: "var(--text-muted)" }} />
                      </button>
                      {isExpanded && (
                        <div className="px-4 pb-4 space-y-3 border-t" style={{ borderColor: "var(--border)" }}>
                          <div className="pt-3 space-y-3">
                            <Row label="任务名称">
                              <Input value={job.name} onChange={(v) => { const u = [...schedJobs]; u[idx] = { ...u[idx], name: v }; setSchedJobs(u); }} />
                            </Row>
                            <Row label="Cron 表达式">
                              <Input value={job.cron} onChange={(v) => { const u = [...schedJobs]; u[idx] = { ...u[idx], cron: v }; setSchedJobs(u); }} />
                              <span className="text-[11px]" style={{ color: "var(--text-muted)" }}>
                                格式：分(0-59) 时(0-23) 日(1-31) 月(1-12) 周(0-7, 1-5=工作日)
                              </span>
                            </Row>
                            <Row label="关联技能">
                              <select
                                value={job.skill_ref ?? ""}
                                onChange={(e) => {
                                  const u = [...schedJobs];
                                  const skillName = e.target.value;
                                  u[idx] = { ...u[idx], skill_ref: skillName || undefined };
                                  if (skillName) {
                                    const skill = userSkills.find(s => s.name === skillName);
                                    if (skill) u[idx].action = skill.content;
                                  }
                                  setSchedJobs(u);
                                }}
                                className="w-full rounded-md px-3 py-1.5 text-sm outline-none appearance-none cursor-pointer"
                                style={{ background: "var(--bg-primary)", border: "1px solid var(--border)", color: "var(--text-primary)" }}
                              >
                                <option value="">不关联（使用自定义指令）</option>
                                {userSkills.map((s) => (
                                  <option key={s.name} value={s.name}>{s.name}</option>
                                ))}
                              </select>
                              <span className="text-[11px]" style={{ color: "var(--text-muted)" }}>选择技能后，指令内容会自动同步。也可在「自定义技能」中管理。</span>
                            </Row>
                            <Row label="执行指令">
                              <textarea
                                value={job.action}
                                onChange={(e) => { const u = [...schedJobs]; u[idx] = { ...u[idx], action: e.target.value }; setSchedJobs(u); }}
                                rows={5}
                                className="w-full rounded-md px-3 py-1.5 text-sm outline-none resize-y"
                                style={{ background: "var(--bg-primary)", border: "1px solid var(--border)", color: "var(--text-primary)" }}
                              />
                            </Row>
                          </div>
                          <div className="flex items-center justify-between">
                            <div className="flex items-center gap-2">
                              <span className="text-xs" style={{ color: "var(--text-muted)" }}>自动执行</span>
                              <Toggle on={job.auto_execute} onChange={(v) => { const u = [...schedJobs]; u[idx] = { ...u[idx], auto_execute: v }; setSchedJobs(u); }} />
                            </div>
                            <button onClick={() => { setSchedJobs(schedJobs.filter((_, i) => i !== idx)); setExpandedJob(null); }}
                              className="flex items-center gap-1 px-2 py-1 rounded text-xs" style={{ color: "var(--danger)" }}>
                              <Trash2 size={12} /> 删除
                            </button>
                          </div>
                        </div>
                      )}
                    </div>
                  );})}

                  <Card title="添加新任务" desc="">
                    <Row label="任务名称">
                      <Input value={newJobName} onChange={setNewJobName} placeholder="如：早盘分析" />
                    </Row>
                    <Row label="Cron 表达式">
                      <Input value={newJobCron} onChange={setNewJobCron} placeholder="35 9 * * 1-5（分 时 日 月 周）" />
                      <div className="mt-1.5 p-2.5 rounded text-[11px] leading-relaxed space-y-1.5" style={{ background: "var(--bg-primary)", border: "1px solid var(--border)", color: "var(--text-muted)" }}>
                        <p className="font-medium" style={{ color: "var(--text-secondary)" }}>Cron 格式：分 时 日 月 周（5个字段，空格分隔）</p>
                        <div className="grid grid-cols-5 gap-1 text-center">
                          {[
                            { f: "分", r: "0-59" }, { f: "时", r: "0-23" },
                            { f: "日", r: "1-31" }, { f: "月", r: "1-12" },
                            { f: "周", r: "0-7*" },
                          ].map((c) => (
                            <div key={c.f} className="px-1 py-0.5 rounded" style={{ background: "var(--bg-tertiary)" }}>
                              <div className="font-medium">{c.f}</div><div>{c.r}</div>
                            </div>
                          ))}
                        </div>
                        <p>* 周: 0和7=周日, 1-5=周一至周五</p>
                        <div className="space-y-0.5">
                          <p className="font-medium" style={{ color: "var(--text-secondary)" }}>常用示例（点击填入）：</p>
                          {[
                            { expr: "35 9 * * 1-5", desc: "工作日 9:35" },
                            { expr: "0 8 * * *", desc: "每天 8:00" },
                            { expr: "*/30 * * * *", desc: "每 30 分钟" },
                            { expr: "0 9,18 * * *", desc: "每天 9:00 和 18:00" },
                            { expr: "0 22 * * 0", desc: "每周日 22:00" },
                          ].map((ex) => (
                            <button key={ex.expr} onClick={() => setNewJobCron(ex.expr)}
                              className="block w-full text-left px-1.5 py-0.5 rounded transition-colors hover:opacity-80"
                              style={{ background: newJobCron === ex.expr ? "var(--accent)" : "transparent", color: newJobCron === ex.expr ? "white" : "inherit" }}>
                              <code>{ex.expr}</code> → {ex.desc}
                            </button>
                          ))}
                        </div>
                      </div>
                    </Row>
                    <Row label="关联技能">
                      <select
                        value={newJobSkillRef}
                        onChange={(e) => {
                          setNewJobSkillRef(e.target.value);
                          if (e.target.value) {
                            const skill = userSkills.find(s => s.name === e.target.value);
                            if (skill) setNewJobAction(skill.content);
                          }
                        }}
                        className="w-full rounded-md px-3 py-1.5 text-sm outline-none appearance-none cursor-pointer"
                        style={{ background: "var(--bg-primary)", border: "1px solid var(--border)", color: "var(--text-primary)" }}
                      >
                        <option value="">不关联（手动输入指令）</option>
                        {userSkills.map((s) => (
                          <option key={s.name} value={s.name}>{s.name}</option>
                        ))}
                      </select>
                    </Row>
                    <Row label="执行指令">
                      <textarea
                        value={newJobAction}
                        onChange={(e) => setNewJobAction(e.target.value)}
                        placeholder="你是资深投资分析师。请生成早盘分析报告..."
                        rows={3}
                        className="w-full rounded-md px-3 py-1.5 text-sm outline-none resize-y"
                        style={{ background: "var(--bg-primary)", border: "1px solid var(--border)", color: "var(--text-primary)" }}
                      />
                    </Row>
                    <div className="flex gap-2">
                      <button
                        onClick={() => {
                          if (!newJobName || !newJobCron || !newJobAction) return;
                          setSchedJobs([...schedJobs, { name: newJobName, cron: newJobCron, action: newJobAction, auto_execute: true, skill_ref: newJobSkillRef || undefined }]);
                          setNewJobName(""); setNewJobCron(""); setNewJobAction(""); setNewJobSkillRef("");
                        }}
                        disabled={!newJobName || !newJobCron || !newJobAction}
                        className="flex items-center gap-1 px-3 py-1.5 rounded-md text-xs text-white transition-colors disabled:opacity-50"
                        style={{ background: "var(--accent)" }}
                      >
                        <Plus size={12} /> 添加
                      </button>
                      {schedJobs.length === 0 && (
                        <button
                          onClick={() => setSchedJobs([
                            { name: "早盘分析报告", cron: "35 9 * * 1-5", action: "请生成今日早盘分析报告。用 get_market_price 查询 A股（上证指数/沪深300/创业板）、港股（恒生指数）、加密货币（BTC/ETH）、黄金/白银/原油的实时行情，用 search_web 搜索市场新闻，给出各板块操作建议。", auto_execute: true, skill_ref: "投资分析师" },
                            { name: "午盘总结", cron: "50 11 * * 1-5", action: "请生成午盘总结。查询 A股和港股实时行情，回顾上午走势，给出下午盘操作建议。", auto_execute: true, skill_ref: "投资分析师" },
                            { name: "尾盘预警", cron: "50 14 * * 1-5", action: "请生成尾盘预警报告。查询 A股全天走势，分析尾盘资金流向，给出收盘预判和明日展望。", auto_execute: true, skill_ref: "投资分析师" },
                            { name: "收盘日报", cron: "45 15 * * 1-5", action: "请生成收盘日报。查询 A股/港股收盘数据、加密货币/黄金白银原油实时行情、美股盘前信号，给出次日策略。", auto_execute: true, skill_ref: "投资分析师" },
                            { name: "夜盘分析", cron: "30 22 * * 1-5", action: "请生成夜盘分析报告。查询美股开盘表现、加密货币/黄金白银夜盘实时行情，分析对次日亚太市场影响。", auto_execute: true, skill_ref: "投资分析师" },
                            { name: "科技日报", cron: "0 8 * * *", action: "请生成今日科技圈早报。用 search_web 搜索最新动态，覆盖 AI/大模型、加密货币/Web3、机器人/自动化、科技公司动态，给出职业发展建议。", auto_execute: true, skill_ref: "科技日报" },
                          ])}
                          className="px-3 py-1.5 rounded-md text-xs transition-colors"
                          style={{ background: "var(--bg-tertiary)", color: "var(--text-secondary)" }}
                        >
                          加载默认（投资+科技日报）
                        </button>
                      )}
                    </div>
                  </Card>
                </>
              )}
            </Section>
          )}

          {/* ============ 自定义技能 ============ */}
          {activeTab === "skills" && (
            <Section title="自定义技能 / 指令" desc="添加命名技能来个性化 AI 助理行为。技能可在定时任务中直接引用，也会注入到系统提示词中。">
              <Card title="全局自定义指令" desc="始终生效的基础指令（不需要命名，直接生效）">
                <textarea
                  value={customInstructions}
                  onChange={(e) => setCustomInstructions(e.target.value)}
                  placeholder={"示例：\n- 我是一名程序员，关注AI和加密货币领域\n- 回复风格简洁专业，不要废话"}
                  rows={4}
                  className="w-full rounded-md px-3 py-2 text-sm outline-none resize-y"
                  style={{ background: "var(--bg-primary)", border: "1px solid var(--border)", color: "var(--text-primary)" }}
                />
              </Card>

              {userSkills.map((skill, idx) => {
                const isExpanded = expandedSkill === idx;
                return (
                <div key={idx} className="rounded-lg" style={{ background: "var(--bg-secondary)", border: "1px solid var(--border)" }}>
                  <button
                    className="w-full flex items-center justify-between p-4 text-left"
                    onClick={() => setExpandedSkill(isExpanded ? null : idx)}
                  >
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <Sparkles size={14} style={{ color: "var(--accent)" }} />
                        <h4 className="text-sm font-medium">{skill.name}</h4>
                        {skill.always_on && <span className="px-1.5 py-0.5 rounded text-[10px]" style={{ background: "var(--accent)", color: "white", opacity: 0.8 }}>常驻</span>}
                      </div>
                      {!isExpanded && (
                        <p className="text-xs mt-1 truncate" style={{ color: "var(--text-muted)" }}>
                          {skill.content.slice(0, 80)}{skill.content.length > 80 ? "..." : ""}
                        </p>
                      )}
                    </div>
                    <ChevronRight size={14} className="shrink-0 ml-2 transition-transform" style={{ transform: isExpanded ? "rotate(90deg)" : "none", color: "var(--text-muted)" }} />
                  </button>
                  {isExpanded && (
                    <div className="px-4 pb-4 space-y-3 border-t" style={{ borderColor: "var(--border)" }}>
                      <div className="pt-3 space-y-3">
                        <Row label="技能名称">
                          <Input value={skill.name} onChange={(v) => { const u = [...userSkills]; u[idx] = { ...u[idx], name: v }; setUserSkills(u); }} />
                        </Row>
                        <Row label="触发关键词">
                          <Input
                            value={(skill.keywords ?? []).join(", ")}
                            onChange={(v) => { const u = [...userSkills]; u[idx] = { ...u[idx], keywords: v.split(",").map(s => s.trim()).filter(Boolean) }; setUserSkills(u); }}
                          />
                          <span className="text-[11px]" style={{ color: "var(--text-muted)" }}>
                            逗号分隔。用户消息中包含任一关键词时，此技能会自动激活。留空则根据技能名自动提取。
                          </span>
                        </Row>
                        <div className="flex items-center gap-2">
                          <span className="text-xs" style={{ color: "var(--text-muted)" }}>始终激活</span>
                          <Toggle on={skill.always_on ?? false} onChange={(v) => { const u = [...userSkills]; u[idx] = { ...u[idx], always_on: v }; setUserSkills(u); }} />
                          <span className="text-[11px]" style={{ color: "var(--text-muted)" }}>开启后每次对话都会注入此技能（适合"简洁风格"等全局偏好）</span>
                        </div>
                        <Row label="技能内容">
                          <textarea
                            value={skill.content}
                            onChange={(e) => { const u = [...userSkills]; u[idx] = { ...u[idx], content: e.target.value }; setUserSkills(u); }}
                            rows={6}
                            className="w-full rounded-md px-3 py-1.5 text-sm outline-none resize-y"
                            style={{ background: "var(--bg-primary)", border: "1px solid var(--border)", color: "var(--text-primary)" }}
                          />
                        </Row>
                      </div>
                      <div className="flex justify-between items-center">
                        <div className="text-[11px]" style={{ color: "var(--text-muted)" }}>
                          文件: <code className="px-1 py-0.5 rounded" style={{ background: "var(--bg-tertiary)" }}>{skill.name}.md</code>
                        </div>
                        <button onClick={async () => {
                          await invoke("delete_skill", { name: skill.name }).catch(() => {});
                          setUserSkills(userSkills.filter((_, i) => i !== idx));
                          setExpandedSkill(null);
                        }}
                          className="flex items-center gap-1 px-2 py-1 rounded text-xs" style={{ color: "var(--danger)" }}>
                          <Trash2 size={12} /> 删除技能
                        </button>
                      </div>
                    </div>
                  )}
                </div>
              );})}

              <div
                className="rounded-lg p-4 space-y-4 transition-colors"
                style={{
                  background: dragOver ? "var(--bg-tertiary)" : "var(--bg-secondary)",
                  border: dragOver ? "2px dashed var(--accent)" : "1px solid var(--border)",
                }}
                onDragOver={(e) => { e.preventDefault(); setDragOver(true); }}
                onDragLeave={() => setDragOver(false)}
                onDrop={async (e) => { e.preventDefault(); setDragOver(false); await handleSkillFiles(e.dataTransfer.files); }}
              >
                <div>
                  <h4 className="text-sm font-medium">添加新技能</h4>
                  <p className="text-[11px] mt-0.5" style={{ color: "var(--text-muted)" }}>手动编写、上传 .md 文件，或拖放文件到此区域</p>
                </div>

                {dragOver && (
                  <div className="flex items-center justify-center py-8 rounded-lg" style={{ border: "2px dashed var(--accent)", background: "var(--bg-primary)" }}>
                    <div className="text-center">
                      <Upload size={24} className="mx-auto mb-2" style={{ color: "var(--accent)" }} />
                      <p className="text-sm" style={{ color: "var(--accent)" }}>释放以导入技能文件</p>
                    </div>
                  </div>
                )}

                {!dragOver && (
                  <>
                    <input ref={fileInputRef} type="file" accept=".md,.txt" multiple className="hidden"
                      onChange={async (e) => { await handleSkillFiles(e.target.files); if (fileInputRef.current) fileInputRef.current.value = ""; }} />

                    <Row label="技能名称">
                      <Input value={newSkillName} onChange={setNewSkillName} placeholder="如：投资分析师、代码审查员、周报生成" />
                    </Row>
                    <Row label="触发关键词">
                      <Input value={newSkillKeywords} onChange={setNewSkillKeywords} placeholder="逗号分隔，如：投资, 股票, A股, 行情" />
                      <span className="text-[11px]" style={{ color: "var(--text-muted)" }}>
                        用户消息中包含任一关键词时自动激活此技能。留空则根据技能名自动提取。
                      </span>
                    </Row>
                    <div className="flex items-center gap-2">
                      <span className="text-xs" style={{ color: "var(--text-muted)" }}>始终激活</span>
                      <Toggle on={newSkillAlwaysOn} onChange={setNewSkillAlwaysOn} />
                      <span className="text-[11px]" style={{ color: "var(--text-muted)" }}>开启后每次对话都注入（适合全局偏好类技能）</span>
                    </div>
                    <Row label="技能内容">
                      <textarea
                        value={newSkillContent}
                        onChange={(e) => setNewSkillContent(e.target.value)}
                        placeholder={"# 角色定义\n你是一位 [角色名称]，擅长 [专业领域]。\n\n# 分析框架\n1. [第一步]：...\n2. [第二步]：...\n\n# 输出要求\n- 格式：列表 / 表格\n- 风格：专业简洁"}
                        rows={8}
                        className="w-full rounded-md px-3 py-1.5 text-sm outline-none resize-y"
                        style={{ background: "var(--bg-primary)", border: "1px solid var(--border)", color: "var(--text-primary)" }}
                      />
                      <p className="text-[11px] mt-1" style={{ color: "var(--text-muted)" }}>
                        提示：好的技能应包含 <strong>角色定义</strong>（你是谁）、<strong>分析框架</strong>（怎么做）、<strong>输出要求</strong>（格式/风格）。可点击下方「预填模板」快速开始。
                      </p>
                    </Row>
                    <div className="flex gap-2 flex-wrap">
                      <button
                        onClick={async () => {
                          if (!newSkillName || !newSkillContent) return;
                          const keywords = newSkillKeywords.split(",").map(s => s.trim()).filter(Boolean);
                          const skill = { name: newSkillName, content: newSkillContent, keywords: keywords.length > 0 ? keywords : undefined, always_on: newSkillAlwaysOn || undefined };
                          await invoke("save_skill", { skill }).catch(() => {});
                          setUserSkills([...userSkills, skill]);
                          setNewSkillName(""); setNewSkillContent(""); setNewSkillKeywords(""); setNewSkillAlwaysOn(false);
                        }}
                        disabled={!newSkillName || !newSkillContent}
                        className="flex items-center gap-1 px-3 py-1.5 rounded-md text-xs text-white transition-colors disabled:opacity-50"
                        style={{ background: "var(--accent)" }}
                      >
                        <Plus size={12} /> 添加技能
                      </button>
                      <button
                        onClick={() => fileInputRef.current?.click()}
                        className="flex items-center gap-1 px-3 py-1.5 rounded-md text-xs transition-colors"
                        style={{ background: "var(--bg-tertiary)", color: "var(--text-secondary)", border: "1px solid var(--border)" }}
                      >
                        <Upload size={12} /> 上传 .md 文件
                      </button>
                      <button
                        onClick={() => {
                          setNewSkillName(newSkillName || "我的新技能");
                          setNewSkillKeywords(newSkillKeywords || "关键词1, 关键词2, 关键词3");
                          setNewSkillContent([
                            "# 角色定义",
                            "你是一位 [角色名称]，擅长 [专业领域]。",
                            "",
                            "# 分析框架",
                            "分析时请按以下步骤：",
                            "1. 首先 [第一步]：描述具体分析维度",
                            "2. 然后 [第二步]：描述下一步操作",
                            "3. 最后 [第三步]：给出结论或建议",
                            "",
                            "# 输出要求",
                            "- 格式：使用列表和小标题，层次清晰",
                            "- 风格：专业但易懂，避免废话",
                            "- 长度：控制在 500 字以内",
                            "",
                            "# 特别注意",
                            "- 数据要基于工具返回的实时结果，不要编造",
                            "- 给出明确的建议，不要模棱两可",
                          ].join("\n"));
                        }}
                        className="px-3 py-1.5 rounded-md text-xs transition-colors"
                        style={{ background: "var(--bg-tertiary)", color: "var(--text-secondary)", border: "1px solid var(--border)" }}
                      >
                        预填模板
                      </button>
                    </div>
                  </>
                )}
              </div>

              <Card title="快速添加模板" desc="一键创建预设技能（含关键词，自动按需激活）">
                <div className="flex flex-wrap gap-2">
                  {[
                    { name: "投资分析师", keywords: ["投资", "股票", "A股", "港股", "美股", "行情", "持仓", "盘", "策略", "黄金", "原油", "加仓", "减仓"], content: "你同时是一位资深投资分析师。分析行情时：\n1. 先看宏观（政策/利率/地缘政治）\n2. 再看技术面（支撑/压力/成交量）\n3. 给出明确的操作建议（建仓/加仓/减仓/观望）和止损位\n\n风格专业但易懂。数据必须来自 get_market_price 工具返回的实时结果。" },
                    { name: "科技圈观察员", keywords: ["科技", "AI", "大模型", "机器人", "芯片", "Web3", "加密货币", "技术动态"], content: "你同时是一位科技行业分析师。关注领域：AI大模型、加密货币/Web3、机器人/自动化、芯片/半导体。\n\n分析时注意：\n- 技术突破的商业化前景\n- 对行业格局的影响\n- 投资和职业机会" },
                    { name: "简洁风格", keywords: ["风格"], always_on: true, content: "回复风格要求：\n1. 直接给结论，不要铺垫\n2. 用列表而非段落\n3. 数据要精确\n4. 每次回复控制在 200 字以内" },
                    { name: "晨间投资简报", keywords: ["投资简报", "晨报", "早盘", "持仓分析"], content: "你是一位资深投资分析师。请按以下框架生成今日投资简报：\n\n## 输出格式\n📊 晨间投资简报 [日期]\n\n### 一句话结论\n[今日最重要的一件事]\n\n### 持仓分析\n对每个持仓标的：\n- 当前价格（用 get_market_price 查询）\n- 过去24h重大消息（用 search_web 搜索）\n- 今日操作建议：持有/加仓/减仓/止损\n\n### 宏观环境\n- 美联储/央行政策动向\n- 重要经济数据\n- 市场情绪指标\n\n### 今日关注点\n1. 🔴 [最重要] ...\n2. 🟡 [次重要] ...\n3. 🟢 [可选关注] ...\n\n⚠️ 以上分析仅供参考，不构成投资建议。" },
                    { name: "科技日报", keywords: ["科技日报", "科技早报", "技术动态"], content: "你是科技领域资深分析师。请生成今日科技圈早报。\n\n覆盖（用 search_web 搜索最新动态）：\n1. AI/大模型最新进展\n2. 加密货币/Web3 动态\n3. 机器人/自动化技术\n4. 值得关注的科技公司动态\n\n最后给出 1-2 条职业发展建议。" },
                    { name: "选题推荐", keywords: ["选题", "内容策划", "自媒体"], content: "你是一位内容策划专家，专注技术类自媒体。\n\n## 选题来源\n1. 今日科技/AI热点新闻（用 search_web 搜索）\n2. 常青内容（教程、避坑指南）\n\n## 输出格式\n💡 今日选题推荐\n\n### 热点类（时效性强）\n1. **标题**: [建议标题]\n   平台: B站/小红书/公众号\n   关键词: [SEO关键词]\n   理由: [为什么现在做这个]\n\n### 常青类\n1. **标题**: [建议标题]\n   ...\n\n### 建议优先做\n[选一个最值得做的，给出理由]" },
                    { name: "周度复盘", keywords: ["周报", "复盘", "周度"], content: "你是个人效率教练。请生成本周复盘报告：\n\n📋 周度综合复盘\n\n### 一、投资周报\n- 本周持仓盈亏汇总\n- 下周关键事件和操作计划\n\n### 二、学习周报\n- 本周学习收获（一句话）\n- 下周学习目标\n\n### 三、综合建议\n按优先级列出下周最重要的 3 件事：\n1. 🔴 ...\n2. 🟡 ...\n3. 🟢 ..." },
                  ].filter(tpl => !userSkills.some(s => s.name === tpl.name)).map((tpl) => (
                    <button
                      key={tpl.name}
                      onClick={async () => {
                        await invoke("save_skill", { skill: tpl }).catch(() => {});
                        setUserSkills([...userSkills, tpl]);
                      }}
                      className="px-3 py-1.5 rounded-md text-xs transition-colors"
                      style={{ background: "var(--bg-tertiary)", color: "var(--text-secondary)", border: "1px solid var(--border)" }}
                    >
                      + {tpl.name}
                    </button>
                  ))}
                </div>
              </Card>

              <div className="p-3 rounded-lg text-xs space-y-1.5" style={{ background: "var(--bg-secondary)", border: "1px solid var(--border)", color: "var(--text-muted)" }}>
                <p className="font-medium" style={{ color: "var(--text-secondary)" }}>存储说明</p>
                <p><strong>技能文件:</strong> 每个技能保存为独立 <code className="px-1 py-0.5 rounded text-[11px]" style={{ background: "var(--bg-tertiary)" }}>.md</code> 文件，可直接用编辑器修改</p>
                <p><strong>技能目录:</strong> <code className="px-1 py-0.5 rounded text-[11px]" style={{ background: "var(--bg-tertiary)" }}>{skillsDir || "加载中..."}</code></p>
                <p><strong>主配置:</strong> 定时任务、模型等保存在 <code className="px-1 py-0.5 rounded text-[11px]" style={{ background: "var(--bg-tertiary)" }}>auto-crab.toml</code>（同目录）</p>
                <p><strong>全局指令:</strong> 始终注入到系统提示词，影响所有对话和定时任务</p>
                <p><strong>命名技能:</strong> 可在定时任务中通过下拉选择引用，也会注入到系统提示词</p>
              </div>
            </Section>
          )}

          {/* ============ 密钥管理 ============ */}
          {activeTab === "credentials" && (
            <Section title="密钥管理" desc="分类管理所有 API 密钥。系统密钥链中的凭据安全加密存储，搜索 API 密钥保存在配置文件中。">
              {/* ── 大模型 API ── */}
              <Card title="🤖 大模型 API" desc="AI 对话所需的 LLM 服务密钥（存储在系统密钥链中）">
                <div className="space-y-1.5">
                  {[
                    { key: "deepseek", label: "DeepSeek" },
                    { key: "dashscope", label: "通义千问 (DashScope)" },
                    { key: "zhipu", label: "智谱 (GLM)" },
                    { key: "moonshot", label: "Kimi (Moonshot)" },
                    { key: "openai", label: "OpenAI" },
                    { key: "anthropic", label: "Anthropic (Claude)" },
                  ].map(({ key: k, label }) => (
                    <CredentialRow key={k} name={k} exists={credentialStatuses[k]} onEdit={(name) => {
                      setKeyName(name);
                      setApiKeyInput("");
                      setSaveMsg("");
                    }} />
                  ))}
                </div>
              </Card>

              {/* ── 搜索引擎 API ── */}
              <Card title="🔍 搜索引擎 API" desc="用于全球搜索，未配置时使用免费的 DuckDuckGo 爬虫兜底">
                <div className="space-y-3">
                  <div className="flex items-center justify-between p-2 rounded" style={{ background: "var(--bg-tertiary)" }}>
                    <div className="flex items-center gap-2">
                      <span className="text-xs font-medium" style={{ color: "var(--text-primary)" }}>Brave Search</span>
                      <span className={`text-[10px] px-1.5 py-0.5 rounded`}
                        style={{ background: braveApiKey ? 'rgba(34,197,94,0.15)' : 'rgba(128,128,128,0.15)', color: braveApiKey ? '#22c55e' : '#9ca3af' }}>
                        {braveApiKey ? '已配置' : '未配置'}
                      </span>
                    </div>
                    {searchStats?.brave && (
                      <div className="text-[11px]" style={{ color: "var(--text-muted)" }}>
                        本月: {searchStats.brave.used}/{searchStats.brave.quota}
                        <span className="ml-1" style={{ color: searchStats.brave.remaining > 100 ? '#22c55e' : '#ef4444' }}>
                          (剩余 {searchStats.brave.remaining})
                        </span>
                      </div>
                    )}
                  </div>
                  <div className="flex items-center justify-between p-2 rounded" style={{ background: "var(--bg-tertiary)" }}>
                    <div className="flex items-center gap-2">
                      <span className="text-xs font-medium" style={{ color: "var(--text-primary)" }}>SerpApi (Google)</span>
                      <span className={`text-[10px] px-1.5 py-0.5 rounded`}
                        style={{ background: serpapiApiKey ? 'rgba(34,197,94,0.15)' : 'rgba(128,128,128,0.15)', color: serpapiApiKey ? '#22c55e' : '#9ca3af' }}>
                        {serpapiApiKey ? '已配置' : '未配置'}
                      </span>
                    </div>
                    {searchStats?.serpapi && (
                      <div className="text-[11px]" style={{ color: "var(--text-muted)" }}>
                        本月: {searchStats.serpapi.used}/{searchStats.serpapi.quota}
                        <span className="ml-1" style={{ color: searchStats.serpapi.remaining > 50 ? '#22c55e' : '#ef4444' }}>
                          (剩余 {searchStats.serpapi.remaining})
                        </span>
                      </div>
                    )}
                  </div>
                  <div className="flex items-center justify-between p-2 rounded" style={{ background: "var(--bg-tertiary)" }}>
                    <div className="flex items-center gap-2">
                      <span className="text-xs font-medium" style={{ color: "var(--text-primary)" }}>Tavily</span>
                      <span className={`text-[10px] px-1.5 py-0.5 rounded`}
                        style={{ background: tavilyApiKey ? 'rgba(34,197,94,0.15)' : 'rgba(128,128,128,0.15)', color: tavilyApiKey ? '#22c55e' : '#9ca3af' }}>
                        {tavilyApiKey ? '已配置' : '未配置'}
                      </span>
                    </div>
                    {searchStats?.tavily && (
                      <div className="text-[11px]" style={{ color: "var(--text-muted)" }}>
                        本月: {searchStats.tavily.used}/{searchStats.tavily.quota}
                        <span className="ml-1" style={{ color: searchStats.tavily.remaining > 100 ? '#22c55e' : '#ef4444' }}>
                          (剩余 {searchStats.tavily.remaining})
                        </span>
                      </div>
                    )}
                  </div>
                </div>
                <p className="text-[11px] mt-2" style={{ color: "var(--text-muted)" }}>
                  搜索 API Key 在「工具权限」标签页配置。搜索优先级：Tavily → SerpApi (Google) → Brave → DuckDuckGo → SearXNG
                </p>
              </Card>

              {/* ── 远程控制 ── */}
              <Card title="🔗 远程控制" desc="飞书、企业微信等集成所需的密钥（存储在系统密钥链中）">
                <div className="space-y-1.5">
                  {[
                    { key: "feishu-secret", label: "飞书 App Secret" },
                    { key: "wechat-work-secret", label: "企业微信 Secret" },
                  ].map(({ key: k }) => (
                    <CredentialRow key={k} name={k} exists={credentialStatuses[k]} onEdit={(name) => {
                      setKeyName(name);
                      setApiKeyInput("");
                      setSaveMsg("");
                    }} />
                  ))}
                </div>
              </Card>

              {/* ── 添加/更新 ── */}
              <Card title="添加 / 更新密钥" desc="将 API Key 安全存储到系统密钥链">
                <Row label="密钥名称">
                  <select
                    value={keyName}
                    onChange={(e) => setKeyName(e.target.value)}
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
                    <option value="">选择密钥...</option>
                    <optgroup label="🤖 大模型 API">
                      <option value="deepseek">deepseek</option>
                      <option value="dashscope">dashscope (通义千问)</option>
                      <option value="zhipu">zhipu (智谱)</option>
                      <option value="moonshot">moonshot (Kimi)</option>
                      <option value="openai">openai</option>
                      <option value="anthropic">anthropic (Claude)</option>
                    </optgroup>
                    <optgroup label="🔗 远程控制">
                      <option value="feishu-secret">feishu-secret (飞书)</option>
                      <option value="wechat-work-secret">wechat-work-secret (企业微信)</option>
                    </optgroup>
                  </select>
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

              <div className="text-xs leading-5 space-y-1 p-3 rounded-lg" style={{ background: "var(--bg-secondary)", border: "1px solid var(--border)", color: "var(--text-muted)" }}>
                <p className="font-medium" style={{ color: "var(--text-secondary)" }}>存储说明</p>
                <p>• <strong>大模型/远程控制密钥</strong>：存储在操作系统级加密存储中（Windows Credential Store），不会出现在配置文件里</p>
                <p>• <strong>搜索 API 密钥</strong>：保存在 <code className="px-1 py-0.5 rounded text-[11px]" style={{ background: "var(--bg-tertiary)" }}>auto-crab.toml</code> 配置文件的 <code>[search]</code> 段</p>
                <p>• 更新密钥只需使用相同的名称重新保存即可覆盖</p>
              </div>
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
