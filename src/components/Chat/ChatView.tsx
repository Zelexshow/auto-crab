import { useState, useRef, useEffect } from "react";
import { Loader2, Sun, Moon, Monitor } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useChatStore } from "../../stores/chatStore";
import { useThemeStore } from "../../stores/themeStore";
import { ChatMessage } from "./ChatMessage";
import { ModelSelector } from "./ModelSelector";

export function ChatView() {
  const [input, setInput] = useState("");
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const streamingMsgId = useRef<string | null>(null);
  const {
    messages, isLoading, addMessage, appendToMessage,
    setMessageDone, setLoading, setStreamId,
  } = useChatStore();
  const { theme, setTheme } = useThemeStore();
  const [selectedModel, setSelectedModel] = useState("deepseek");

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  useEffect(() => {
    console.log("[ChatView] Setting up event listeners");
    const unlisten = listen<{ stream_id: string; delta: string; done: boolean }>(
      "chat-stream-chunk",
      (event) => {
        console.log("[ChatView] chat-stream-chunk received:", event.payload.done, "delta len:", event.payload.delta?.length);
        const { delta, done } = event.payload;
        const msgId = streamingMsgId.current;
        if (msgId) {
          if (delta) appendToMessage(msgId, delta);
          if (done) {
            setMessageDone(msgId);
            setLoading(false);
            setStreamId(null);
            streamingMsgId.current = null;
          }
        } else {
          console.warn("[ChatView] No streamingMsgId, event dropped");
        }
      },
    );

    const unlistenErr = listen<{ stream_id: string; error: string }>(
      "chat-stream-error",
      (event) => {
        const msgId = streamingMsgId.current;
        if (msgId) {
          appendToMessage(msgId, `\n\n⚠️ 错误: ${event.payload.error}`);
          setMessageDone(msgId);
        }
        setLoading(false);
        setStreamId(null);
        streamingMsgId.current = null;
      },
    );

    return () => {
      unlisten.then((fn) => fn());
      unlistenErr.then((fn) => fn());
    };
  }, []);

  const handleSend = async () => {
    const text = input.trim();
    if (!text || isLoading) return;

    setInput("");
    if (textareaRef.current) textareaRef.current.style.height = "auto";
    addMessage({ role: "user", content: text });
    setLoading(true);

    // Build history from existing messages (exclude the one we just added and streaming ones)
    const currentMessages = useChatStore.getState().messages;
    const history = currentMessages
      .filter((m) => !m.isStreaming && m.content && m.role !== "system")
      .slice(0, -1) // exclude the user message we just added (it goes as `message` param)
      .map((m) => ({ role: m.role, content: m.content }));

    const assistantId = addMessage({ role: "assistant", content: "", isStreaming: true });
    streamingMsgId.current = assistantId;

    try {
      const result = await invoke<{ success: boolean; data?: string; error?: string }>(
        "chat_stream_start",
        { message: text, history },
      );

      if (!result.success) {
        appendToMessage(assistantId, result.error || "未知错误");
        setMessageDone(assistantId);
        setLoading(false);
        streamingMsgId.current = null;
      } else {
        setStreamId(result.data || null);
      }
    } catch (e: any) {
      appendToMessage(assistantId, `错误: ${e.toString()}`);
      setMessageDone(assistantId);
      setLoading(false);
      streamingMsgId.current = null;
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  const adjustTextarea = () => {
    const el = textareaRef.current;
    if (el) {
      el.style.height = "auto";
      el.style.height = Math.min(el.scrollHeight, 150) + "px";
    }
  };

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <header
        className="flex items-center justify-between px-5 h-12 border-b shrink-0"
        style={{ borderColor: "var(--border)", background: "var(--bg-secondary)" }}
      >
        <div className="flex items-center gap-2">
          <h1 className="font-semibold text-sm">小蟹</h1>
          <span
            className="text-[11px] px-1.5 py-0.5 rounded"
            style={{ background: "var(--bg-tertiary)", color: "var(--text-muted)" }}
          >
            安全模式
          </span>
        </div>
        <div className="flex items-center gap-2">
          <ModelSelector selected={selectedModel} onSelect={setSelectedModel} />
          <div className="flex items-center gap-0.5">
            {([
              { value: "light" as const, icon: Sun, label: "亮色" },
              { value: "system" as const, icon: Monitor, label: "跟随系统" },
              { value: "dark" as const, icon: Moon, label: "暗色" },
            ]).map(({ value, icon: Icon, label }) => (
              <button
                key={value}
                onClick={() => setTheme(value)}
                title={label}
                className="w-7 h-7 rounded flex items-center justify-center transition-colors"
                style={{
                  background: theme === value ? "var(--bg-tertiary)" : "transparent",
                  color: theme === value ? "var(--accent)" : "var(--text-muted)",
                }}
              >
                <Icon size={14} />
              </button>
            ))}
          </div>
        </div>
      </header>

      {/* Messages area */}
      <div
        className="flex-1 overflow-y-auto px-4 py-4"
        style={{ background: "var(--bg-primary)" }}
      >
        {messages.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full gap-3">
            <div
              className="w-16 h-16 rounded-2xl flex items-center justify-center text-3xl"
              style={{ background: "var(--accent)", color: "#fff" }}
            >
              🦀
            </div>
            <h2 className="text-base font-semibold mt-1">你好，我是小蟹</h2>
            <p style={{ color: "var(--text-muted)" }} className="text-xs text-center max-w-sm">
              你的安全桌面 AI 助理。所有操作经过风险评估，危险操作需要你的确认。
            </p>
            <div className="flex gap-2 mt-3 flex-wrap justify-center">
              {["帮我整理今日任务", "分析代码结构", "写个 Python 脚本"].map((s) => (
                <button
                  key={s}
                  onClick={() => { setInput(s); textareaRef.current?.focus(); }}
                  className="px-3 py-1.5 rounded-md text-xs transition-colors border"
                  style={{
                    background: "var(--bg-secondary)",
                    borderColor: "var(--border)",
                    color: "var(--text-secondary)",
                  }}
                >
                  {s}
                </button>
              ))}
            </div>
          </div>
        ) : (
          <div className="max-w-2xl mx-auto space-y-3">
            {messages.map((msg) => (
              <ChatMessage key={msg.id} message={msg} />
            ))}
            <div ref={messagesEndRef} />
          </div>
        )}
      </div>

      {/* Input area - WeChat style */}
      <div
        className="border-t px-4 py-2.5 shrink-0"
        style={{ borderColor: "var(--border)", background: "var(--bg-secondary)" }}
      >
        <div className="max-w-2xl mx-auto flex items-end gap-2">
          <textarea
            ref={textareaRef}
            value={input}
            onChange={(e) => { setInput(e.target.value); adjustTextarea(); }}
            onKeyDown={handleKeyDown}
            placeholder="输入消息..."
            rows={1}
            className="flex-1 rounded-md px-3 py-2 text-sm leading-6 outline-none resize-none"
            style={{
              background: "var(--bg-primary)",
              color: "var(--text-primary)",
              border: "1px solid var(--border)",
              maxHeight: 150,
            }}
          />
          <button
            onClick={handleSend}
            disabled={!input.trim()}
            className="shrink-0 rounded-md text-xs font-medium transition-colors disabled:opacity-40"
            style={{
              background: "#07c160",
              color: "#fff",
              width: 64,
              height: 38,
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            {isLoading ? (
              <Loader2 size={14} className="animate-spin" style={{ color: "#fff" }} />
            ) : (
              "发送"
            )}
          </button>
        </div>
      </div>
    </div>
  );
}
