import { useState, useRef, useEffect } from "react";
import { Send, Loader2 } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useChatStore } from "../../stores/chatStore";
import { ChatMessage } from "./ChatMessage";

export function ChatView() {
  const [input, setInput] = useState("");
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const {
    messages, isLoading, addMessage, appendToMessage,
    setMessageDone, setLoading, setStreamId,
  } = useChatStore();

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  useEffect(() => {
    const unlisten = listen<{ stream_id: string; delta: string; done: boolean }>(
      "chat-stream-chunk",
      (event) => {
        const { delta, done } = event.payload;
        const assistantMsg = messages.find((m) => m.isStreaming);
        if (assistantMsg) {
          if (delta) appendToMessage(assistantMsg.id, delta);
          if (done) {
            setMessageDone(assistantMsg.id);
            setLoading(false);
            setStreamId(null);
          }
        }
      },
    );

    const unlistenErr = listen<{ stream_id: string; error: string }>(
      "chat-stream-error",
      (event) => {
        const assistantMsg = messages.find((m) => m.isStreaming);
        if (assistantMsg) {
          appendToMessage(assistantMsg.id, `\n\n⚠️ 错误: ${event.payload.error}`);
          setMessageDone(assistantMsg.id);
        }
        setLoading(false);
        setStreamId(null);
      },
    );

    return () => {
      unlisten.then((fn) => fn());
      unlistenErr.then((fn) => fn());
    };
  }, [messages]);

  const handleSend = async () => {
    const text = input.trim();
    if (!text || isLoading) return;

    setInput("");
    addMessage({ role: "user", content: text });
    setLoading(true);

    const assistantId = addMessage({ role: "assistant", content: "", isStreaming: true });

    try {
      const result = await invoke<{ success: boolean; data?: string; error?: string }>(
        "chat_stream_start",
        { message: text },
      );

      if (!result.success) {
        appendToMessage(assistantId, result.error || "未知错误");
        setMessageDone(assistantId);
        setLoading(false);
      } else {
        setStreamId(result.data || null);
      }
    } catch (e: any) {
      appendToMessage(assistantId, `错误: ${e.toString()}`);
      setMessageDone(assistantId);
      setLoading(false);
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
      el.style.height = Math.min(el.scrollHeight, 200) + "px";
    }
  };

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <header
        className="flex items-center justify-between px-6 h-14 border-b shrink-0"
        style={{ borderColor: "var(--border)", background: "var(--bg-primary)" }}
      >
        <div className="flex items-center gap-3">
          <h1 className="font-semibold">Auto Crab</h1>
          <span
            className="text-xs px-2 py-0.5 rounded-full"
            style={{ background: "var(--bg-tertiary)", color: "var(--text-muted)" }}
          >
            安全模式
          </span>
        </div>
      </header>

      {/* Messages */}
      <div className="flex-1 overflow-y-auto px-4 py-6">
        {messages.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full gap-4">
            <div className="text-6xl">🦀</div>
            <h2 className="text-xl font-semibold">你好，我是小蟹</h2>
            <p style={{ color: "var(--text-muted)" }} className="text-sm text-center max-w-md">
              你的安全桌面 AI 助理。所有操作经过风险评估，危险操作需要你的确认。
            </p>
            <div className="flex gap-2 mt-4 flex-wrap justify-center">
              {["帮我整理今日任务", "分析这个项目的代码结构", "写一个 Python 脚本"].map((s) => (
                <button
                  key={s}
                  onClick={() => { setInput(s); textareaRef.current?.focus(); }}
                  className="px-3 py-2 rounded-lg text-sm transition-colors"
                  style={{
                    background: "var(--bg-tertiary)",
                    color: "var(--text-secondary)",
                  }}
                >
                  {s}
                </button>
              ))}
            </div>
          </div>
        ) : (
          <div className="max-w-3xl mx-auto space-y-4">
            {messages.map((msg) => (
              <ChatMessage key={msg.id} message={msg} />
            ))}
            <div ref={messagesEndRef} />
          </div>
        )}
      </div>

      {/* Input area */}
      <div
        className="border-t px-4 py-3 shrink-0"
        style={{ borderColor: "var(--border)", background: "var(--bg-primary)" }}
      >
        <div
          className="max-w-3xl mx-auto flex items-end gap-2 rounded-xl px-4 py-3"
          style={{ background: "var(--bg-secondary)", border: "1px solid var(--border)" }}
        >
          <textarea
            ref={textareaRef}
            value={input}
            onChange={(e) => { setInput(e.target.value); adjustTextarea(); }}
            onKeyDown={handleKeyDown}
            placeholder="输入消息... (Enter 发送, Shift+Enter 换行)"
            rows={1}
            className="flex-1 bg-transparent outline-none resize-none text-sm leading-6"
            style={{ color: "var(--text-primary)", maxHeight: 200 }}
          />
          <button
            onClick={handleSend}
            disabled={isLoading || !input.trim()}
            className="shrink-0 w-8 h-8 rounded-lg flex items-center justify-center transition-colors disabled:opacity-40"
            style={{ background: "var(--accent)", color: "#fff" }}
          >
            {isLoading ? <Loader2 size={16} className="animate-spin" /> : <Send size={16} />}
          </button>
        </div>
        <p className="text-center mt-2 text-xs" style={{ color: "var(--text-muted)" }}>
          Auto Crab v0.1.0 · 所有操作均经过安全审批
        </p>
      </div>
    </div>
  );
}
