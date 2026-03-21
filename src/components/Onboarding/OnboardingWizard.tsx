import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Sparkles, Key, Shield, ChevronRight, Check, Cpu, Cloud } from "lucide-react";

interface OnboardingWizardProps {
  onComplete: () => void;
}

export function OnboardingWizard({ onComplete }: OnboardingWizardProps) {
  const [step, setStep] = useState(0);
  const [provider, setProvider] = useState("dashscope");
  const [apiKey, setApiKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  const providers = [
    { value: "dashscope", label: "通义千问", desc: "阿里云，国内速度快", icon: Cloud },
    { value: "deepseek", label: "DeepSeek", desc: "性价比高，代码能力强", icon: Cloud },
    { value: "zhipu", label: "智谱 GLM", desc: "清华技术背景", icon: Cloud },
    { value: "moonshot", label: "Kimi", desc: "超长上下文", icon: Cloud },
    { value: "openai", label: "OpenAI", desc: "GPT-4o", icon: Cloud },
    { value: "ollama", label: "Ollama 本地", desc: "无需 API Key，需本地安装", icon: Cpu },
  ];

  const handleSaveKey = async () => {
    if (provider === "ollama") {
      onComplete();
      return;
    }
    if (!apiKey.trim()) {
      setError("请输入 API Key");
      return;
    }
    setSaving(true);
    setError("");
    try {
      await invoke("store_credential", { key: provider, secret: apiKey });
      onComplete();
    } catch (e: any) {
      setError("保存失败: " + e.toString());
    }
    setSaving(false);
  };

  const steps = [
    {
      title: "欢迎使用 Auto Crab",
      content: (
        <div className="flex flex-col items-center gap-4 py-6">
          <div
            className="w-20 h-20 rounded-2xl flex items-center justify-center text-4xl shadow-lg"
            style={{ background: "linear-gradient(135deg, #667eea 0%, #764ba2 100%)" }}
          >
            🦀
          </div>
          <h2 className="text-xl font-bold">Auto Crab</h2>
          <p className="text-sm text-center max-w-sm" style={{ color: "var(--text-secondary)" }}>
            你的安全桌面 AI 助理。比 OpenClaw 更安全、更受控、配置更简单。
          </p>
          <div className="space-y-2 text-xs w-full max-w-xs" style={{ color: "var(--text-muted)" }}>
            <div className="flex items-center gap-2">
              <Shield size={14} style={{ color: "var(--success)" }} />
              <span>四级操作风险管控，危险操作需确认</span>
            </div>
            <div className="flex items-center gap-2">
              <Key size={14} style={{ color: "var(--accent)" }} />
              <span>API 密钥加密存储在系统密钥链</span>
            </div>
            <div className="flex items-center gap-2">
              <Sparkles size={14} style={{ color: "var(--warning)" }} />
              <span>支持 7+ 国产/国际模型 + 本地 Ollama</span>
            </div>
          </div>
        </div>
      ),
    },
    {
      title: "选择 AI 模型",
      content: (
        <div className="space-y-2 py-2">
          <p className="text-xs mb-3" style={{ color: "var(--text-muted)" }}>
            选择你要使用的模型提供商。后续可在设置中随时更改。
          </p>
          {providers.map((p) => {
            const Icon = p.icon;
            return (
              <button
                key={p.value}
                onClick={() => setProvider(p.value)}
                className="flex items-center gap-3 w-full rounded-lg px-4 py-3 transition-colors text-left"
                style={{
                  background: provider === p.value ? "var(--accent)" : "var(--bg-secondary)",
                  color: provider === p.value ? "#fff" : "var(--text-primary)",
                  border: `1px solid ${provider === p.value ? "var(--accent)" : "var(--border)"}`,
                }}
              >
                <Icon size={18} />
                <div className="flex-1 min-w-0">
                  <div className="text-sm font-medium">{p.label}</div>
                  <div className="text-[11px] opacity-70">{p.desc}</div>
                </div>
                {provider === p.value && <Check size={16} />}
              </button>
            );
          })}
        </div>
      ),
    },
    {
      title: provider === "ollama" ? "本地模型准备" : "输入 API Key",
      content: (
        <div className="space-y-4 py-2">
          {provider === "ollama" ? (
            <div className="space-y-3">
              <p className="text-sm" style={{ color: "var(--text-secondary)" }}>
                确保 Ollama 已在本地运行：
              </p>
              <div
                className="rounded-lg p-4 text-xs font-mono"
                style={{ background: "var(--bg-secondary)", border: "1px solid var(--border)" }}
              >
                <p style={{ color: "var(--text-muted)" }}># 安装 Ollama 后运行：</p>
                <p className="mt-1">ollama pull qwen2.5:14b</p>
                <p>ollama serve</p>
              </div>
              <p className="text-xs" style={{ color: "var(--text-muted)" }}>
                默认端点: http://localhost:11434
              </p>
            </div>
          ) : (
            <div className="space-y-3">
              <p className="text-sm" style={{ color: "var(--text-secondary)" }}>
                输入你的 {providers.find((p) => p.value === provider)?.label} API Key：
              </p>
              <input
                type="password"
                value={apiKey}
                onChange={(e) => { setApiKey(e.target.value); setError(""); }}
                placeholder="sk-..."
                className="w-full rounded-lg px-4 py-2.5 text-sm outline-none"
                style={{
                  background: "var(--bg-secondary)",
                  border: `1px solid ${error ? "var(--danger)" : "var(--border)"}`,
                  color: "var(--text-primary)",
                }}
                autoFocus
              />
              {error && <p className="text-xs" style={{ color: "var(--danger)" }}>{error}</p>}
              <p className="text-[11px]" style={{ color: "var(--text-muted)" }}>
                密钥将安全存储在系统密钥链中，不会出现在任何配置文件里。
              </p>
            </div>
          )}
        </div>
      ),
    },
  ];

  const isLast = step === steps.length - 1;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center" style={{ background: "rgba(0,0,0,0.5)" }}>
      <div
        className="w-[420px] rounded-2xl shadow-2xl overflow-hidden"
        style={{ background: "var(--bg-primary)", border: "1px solid var(--border)" }}
      >
        {/* Progress */}
        <div className="flex gap-1 px-6 pt-5">
          {steps.map((_, i) => (
            <div
              key={i}
              className="flex-1 h-1 rounded-full transition-colors"
              style={{ background: i <= step ? "var(--accent)" : "var(--bg-tertiary)" }}
            />
          ))}
        </div>

        {/* Content */}
        <div className="px-6 pt-4 pb-2">
          <h3 className="text-base font-semibold mb-2">{steps[step].title}</h3>
          {steps[step].content}
        </div>

        {/* Actions */}
        <div className="flex justify-between px-6 py-4">
          {step > 0 ? (
            <button
              onClick={() => setStep(step - 1)}
              className="px-4 py-2 rounded-lg text-sm transition-colors"
              style={{ color: "var(--text-secondary)", background: "var(--bg-tertiary)" }}
            >
              上一步
            </button>
          ) : (
            <div />
          )}

          <button
            onClick={() => {
              if (isLast) handleSaveKey();
              else setStep(step + 1);
            }}
            disabled={saving}
            className="flex items-center gap-1 px-5 py-2 rounded-lg text-sm text-white transition-colors disabled:opacity-50"
            style={{ background: isLast ? "#07c160" : "var(--accent)" }}
          >
            {saving ? "保存中..." : isLast ? "完成设置" : "下一步"}
            {!isLast && <ChevronRight size={14} />}
          </button>
        </div>
      </div>
    </div>
  );
}
