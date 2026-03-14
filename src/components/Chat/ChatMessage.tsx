import { clsx } from "clsx";
import { User, Bot, Loader2 } from "lucide-react";
import ReactMarkdown from "react-markdown";
import type { Message } from "../../stores/chatStore";

interface ChatMessageProps {
  message: Message;
}

export function ChatMessage({ message }: ChatMessageProps) {
  const isUser = message.role === "user";

  return (
    <div className={clsx("flex gap-3", isUser && "flex-row-reverse")}>
      <div
        className="w-8 h-8 rounded-full flex items-center justify-center shrink-0"
        style={{
          background: isUser ? "var(--accent)" : "var(--bg-tertiary)",
          color: isUser ? "#fff" : "var(--text-secondary)",
        }}
      >
        {isUser ? <User size={16} /> : <Bot size={16} />}
      </div>

      <div
        className={clsx(
          "rounded-2xl px-4 py-3 max-w-[75%] text-sm leading-relaxed",
          isUser ? "rounded-tr-md" : "rounded-tl-md",
        )}
        style={{
          background: isUser ? "var(--user-bubble)" : "var(--assistant-bubble)",
          color: isUser ? "#fff" : "var(--text-primary)",
        }}
      >
        {message.isStreaming && !message.content ? (
          <div className="flex items-center gap-2" style={{ color: "var(--text-muted)" }}>
            <Loader2 size={14} className="animate-spin" />
            <span>思考中...</span>
          </div>
        ) : (
          <div className="prose prose-sm max-w-none dark:prose-invert">
            <ReactMarkdown>{message.content}</ReactMarkdown>
          </div>
        )}

        {message.model && (
          <div className="mt-2 text-xs opacity-60">
            模型: {message.model}
          </div>
        )}
      </div>
    </div>
  );
}
