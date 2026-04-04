import { useState } from "react";
import { ChevronDown, Cpu, Cloud, Check } from "lucide-react";

interface ModelOption {
  provider: string;
  label: string;
  isLocal: boolean;
}

const MODELS: ModelOption[] = [
  { provider: "dashscope", label: "通义千问", isLocal: false },
  { provider: "deepseek", label: "DeepSeek", isLocal: false },
  { provider: "zhipu", label: "智谱 GLM", isLocal: false },
  { provider: "moonshot", label: "Kimi", isLocal: false },
  { provider: "openai", label: "OpenAI", isLocal: false },
  { provider: "anthropic", label: "Claude", isLocal: false },
  { provider: "gemini", label: "Gemini", isLocal: false },
  { provider: "ollama", label: "Ollama 本地", isLocal: true },
];

interface ModelSelectorProps {
  selected: string;
  onSelect: (provider: string) => void;
}

export function ModelSelector({ selected, onSelect }: ModelSelectorProps) {
  const [open, setOpen] = useState(false);
  const current = MODELS.find((m) => m.provider === selected) || MODELS[0];

  return (
    <div className="relative">
      <button
        onClick={() => setOpen(!open)}
        className="flex items-center gap-1.5 px-2 py-1 rounded-md text-xs transition-colors"
        style={{
          background: "var(--bg-tertiary)",
          color: "var(--text-secondary)",
        }}
      >
        {current.isLocal ? <Cpu size={12} /> : <Cloud size={12} />}
        {current.label}
        <ChevronDown size={12} />
      </button>

      {open && (
        <>
          <div className="fixed inset-0 z-40" onClick={() => setOpen(false)} />
          <div
            className="absolute top-full mt-1 right-0 z-50 w-44 rounded-lg shadow-xl overflow-hidden py-1"
            style={{
              background: "var(--bg-primary)",
              border: "1px solid var(--border)",
            }}
          >
            {MODELS.map((m) => (
              <button
                key={m.provider}
                onClick={() => { onSelect(m.provider); setOpen(false); }}
                className="flex items-center justify-between w-full px-3 py-2 text-xs transition-colors hover:opacity-80"
                style={{
                  background: selected === m.provider ? "var(--bg-tertiary)" : "transparent",
                  color: "var(--text-primary)",
                }}
              >
                <span className="flex items-center gap-2">
                  {m.isLocal ? <Cpu size={12} style={{ color: "var(--success)" }} /> : <Cloud size={12} style={{ color: "var(--accent)" }} />}
                  {m.label}
                </span>
                {selected === m.provider && <Check size={12} style={{ color: "var(--accent)" }} />}
              </button>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
