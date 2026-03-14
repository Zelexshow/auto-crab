import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Save, Key, Shield, Bot, Globe, Terminal } from "lucide-react";

export function SettingsView() {
  const [activeTab, setActiveTab] = useState<string>("models");
  const [saving, setSaving] = useState(false);
  const [apiKeyInput, setApiKeyInput] = useState("");
  const [keyName, setKeyName] = useState("");

  const tabs = [
    { id: "models", label: "模型配置", icon: Bot },
    { id: "security", label: "安全设置", icon: Shield },
    { id: "tools", label: "工具权限", icon: Terminal },
    { id: "remote", label: "远程控制", icon: Globe },
    { id: "credentials", label: "密钥管理", icon: Key },
  ];

  const handleSaveKey = async () => {
    if (!keyName || !apiKeyInput) return;
    setSaving(true);
    try {
      await invoke("store_credential", { key: keyName, secret: apiKeyInput });
      setApiKeyInput("");
      setKeyName("");
      alert("密钥已安全保存到系统密钥链");
    } catch (e: any) {
      alert("保存失败: " + e.toString());
    }
    setSaving(false);
  };

  return (
    <div className="flex h-full">
      {/* Settings sidebar */}
      <div
        className="w-48 border-r shrink-0 p-3 space-y-1"
        style={{ borderColor: "var(--border)", background: "var(--bg-secondary)" }}
      >
        <h2 className="text-sm font-semibold px-3 py-2" style={{ color: "var(--text-muted)" }}>
          设置
        </h2>
        {tabs.map((tab) => {
          const Icon = tab.icon;
          return (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className="flex items-center gap-2 w-full rounded-lg px-3 py-2 text-sm transition-colors"
              style={{
                background: activeTab === tab.id ? "var(--bg-tertiary)" : "transparent",
                color: activeTab === tab.id ? "var(--text-primary)" : "var(--text-secondary)",
              }}
            >
              <Icon size={16} />
              {tab.label}
            </button>
          );
        })}
      </div>

      {/* Settings content */}
      <div className="flex-1 overflow-y-auto p-8">
        <div className="max-w-2xl">
          {activeTab === "models" && (
            <div className="space-y-6">
              <div>
                <h3 className="text-lg font-semibold mb-1">模型配置</h3>
                <p className="text-sm" style={{ color: "var(--text-muted)" }}>
                  配置 AI 模型提供商。支持通义千问、DeepSeek、智谱 GLM、Kimi、OpenAI、Claude 以及本地 Ollama。
                </p>
              </div>

              <div className="space-y-4">
                <SettingCard title="主模型" desc="日常对话和任务使用的默认模型">
                  <ProviderSelect />
                </SettingCard>

                <SettingCard title="回退模型" desc="主模型不可用时自动切换，建议配置本地 Ollama 模型">
                  <ProviderSelect />
                </SettingCard>

                <SettingCard title="编码模型" desc="代码生成和分析专用模型，建议使用 DeepSeek Coder">
                  <ProviderSelect />
                </SettingCard>
              </div>
            </div>
          )}

          {activeTab === "security" && (
            <div className="space-y-6">
              <div>
                <h3 className="text-lg font-semibold mb-1">安全设置</h3>
                <p className="text-sm" style={{ color: "var(--text-muted)" }}>
                  控制 Auto Crab 的安全行为。默认启用最高安全级别。
                </p>
              </div>
              <SettingCard title="操作审批" desc="所有风险操作需要用户确认后才能执行">
                <div className="flex items-center gap-2 text-sm" style={{ color: "var(--success)" }}>
                  <Shield size={16} /> 已启用（不可关闭）
                </div>
              </SettingCard>
              <SettingCard title="自动锁定" desc="空闲一段时间后自动锁定，需要重新验证">
                <span className="text-sm">15 分钟</span>
              </SettingCard>
            </div>
          )}

          {activeTab === "tools" && (
            <div className="space-y-6">
              <div>
                <h3 className="text-lg font-semibold mb-1">工具权限</h3>
                <p className="text-sm" style={{ color: "var(--text-muted)" }}>
                  控制 AI 助理可以使用的工具和访问范围。
                </p>
              </div>
              <SettingCard title="Shell 执行" desc="允许执行命令行命令">
                <span className="text-sm" style={{ color: "var(--warning)" }}>
                  已启用 (git, npm, pnpm, python, cargo, node)
                </span>
              </SettingCard>
              <SettingCard title="文件访问" desc="AI 可访问的目录范围">
                <span className="text-sm" style={{ color: "var(--text-muted)" }}>当前工作目录</span>
              </SettingCard>
              <SettingCard title="网络访问" desc="AI 是否可以发起外部网络请求">
                <span className="text-sm" style={{ color: "var(--text-muted)" }}>已启用</span>
              </SettingCard>
            </div>
          )}

          {activeTab === "remote" && (
            <div className="space-y-6">
              <div>
                <h3 className="text-lg font-semibold mb-1">远程控制</h3>
                <p className="text-sm" style={{ color: "var(--text-muted)" }}>
                  通过飞书或企业微信远程控制 Auto Crab。所有远程操作同样经过安全审批。
                </p>
              </div>
              <SettingCard title="飞书 Bot" desc="配置飞书应用 ID 和密钥以启用飞书远程控制">
                <span className="text-sm" style={{ color: "var(--text-muted)" }}>未配置</span>
              </SettingCard>
              <SettingCard title="企业微信 Bot" desc="配置企业微信应用以启用微信远程控制">
                <span className="text-sm" style={{ color: "var(--text-muted)" }}>未配置</span>
              </SettingCard>
            </div>
          )}

          {activeTab === "credentials" && (
            <div className="space-y-6">
              <div>
                <h3 className="text-lg font-semibold mb-1">密钥管理</h3>
                <p className="text-sm" style={{ color: "var(--text-muted)" }}>
                  所有 API 密钥安全存储在系统密钥链中（Windows Credential Store / macOS Keychain），不存储在配置文件中。
                </p>
              </div>
              <SettingCard title="添加密钥" desc="将 API Key 存储到系统密钥链">
                <div className="space-y-3">
                  <div>
                    <label className="text-xs mb-1 block" style={{ color: "var(--text-muted)" }}>
                      密钥名称（如 dashscope, deepseek, openai）
                    </label>
                    <input
                      type="text"
                      value={keyName}
                      onChange={(e) => setKeyName(e.target.value)}
                      className="w-full rounded-lg px-3 py-2 text-sm outline-none"
                      style={{
                        background: "var(--bg-primary)",
                        border: "1px solid var(--border)",
                        color: "var(--text-primary)",
                      }}
                      placeholder="dashscope"
                    />
                  </div>
                  <div>
                    <label className="text-xs mb-1 block" style={{ color: "var(--text-muted)" }}>
                      API Key
                    </label>
                    <input
                      type="password"
                      value={apiKeyInput}
                      onChange={(e) => setApiKeyInput(e.target.value)}
                      className="w-full rounded-lg px-3 py-2 text-sm outline-none"
                      style={{
                        background: "var(--bg-primary)",
                        border: "1px solid var(--border)",
                        color: "var(--text-primary)",
                      }}
                      placeholder="sk-..."
                    />
                  </div>
                  <button
                    onClick={handleSaveKey}
                    disabled={saving || !keyName || !apiKeyInput}
                    className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm text-white transition-colors disabled:opacity-40"
                    style={{ background: "var(--accent)" }}
                  >
                    <Save size={14} />
                    {saving ? "保存中..." : "安全保存到密钥链"}
                  </button>
                </div>
              </SettingCard>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function SettingCard({ title, desc, children }: { title: string; desc: string; children: React.ReactNode }) {
  return (
    <div
      className="rounded-xl p-5"
      style={{
        background: "var(--bg-secondary)",
        border: "1px solid var(--border)",
      }}
    >
      <h4 className="font-medium text-sm mb-1">{title}</h4>
      <p className="text-xs mb-3" style={{ color: "var(--text-muted)" }}>{desc}</p>
      {children}
    </div>
  );
}

function ProviderSelect() {
  const providers = [
    { value: "dashscope", label: "通义千问 (DashScope)" },
    { value: "deepseek", label: "DeepSeek" },
    { value: "zhipu", label: "智谱 GLM" },
    { value: "moonshot", label: "月之暗面 Kimi" },
    { value: "openai", label: "OpenAI" },
    { value: "anthropic", label: "Anthropic Claude" },
    { value: "ollama", label: "Ollama (本地)" },
  ];

  return (
    <select
      className="w-full rounded-lg px-3 py-2 text-sm outline-none"
      style={{
        background: "var(--bg-primary)",
        border: "1px solid var(--border)",
        color: "var(--text-primary)",
      }}
    >
      <option value="">选择模型提供商...</option>
      {providers.map((p) => (
        <option key={p.value} value={p.value}>{p.label}</option>
      ))}
    </select>
  );
}
