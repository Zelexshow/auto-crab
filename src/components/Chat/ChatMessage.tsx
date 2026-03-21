import { useState } from "react";
import { clsx } from "clsx";
import { Loader2, Copy, Check } from "lucide-react";
import ReactMarkdown from "react-markdown";
import { Prism as SyntaxHighlighter } from "react-syntax-highlighter";
import { oneDark } from "react-syntax-highlighter/dist/esm/styles/prism";
import type { Message } from "../../stores/chatStore";

interface ChatMessageProps {
  message: Message;
}

function CodeBlock({ language, value }: { language: string; value: string }) {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    navigator.clipboard.writeText(value);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="relative group my-2 rounded-lg overflow-hidden" style={{ border: "1px solid var(--border)" }}>
      {/* Header bar */}
      <div
        className="flex items-center justify-between px-3 py-1"
        style={{ background: "rgba(0,0,0,0.3)" }}
      >
        <span className="text-[10px] uppercase tracking-wider" style={{ color: "rgba(255,255,255,0.5)" }}>
          {language || "code"}
        </span>
        <button
          onClick={handleCopy}
          className="flex items-center gap-1 text-[10px] px-1.5 py-0.5 rounded transition-colors"
          style={{ color: "rgba(255,255,255,0.6)" }}
        >
          {copied ? <><Check size={10} /> 已复制</> : <><Copy size={10} /> 复制</>}
        </button>
      </div>
      <SyntaxHighlighter
        language={language || "text"}
        style={oneDark}
        customStyle={{
          margin: 0,
          padding: "12px 16px",
          fontSize: "12px",
          lineHeight: "1.5",
          borderRadius: 0,
          background: "#1e1e2e",
        }}
        wrapLongLines
      >
        {value}
      </SyntaxHighlighter>
    </div>
  );
}

export function ChatMessage({ message }: ChatMessageProps) {
  const isUser = message.role === "user";
  const [copied, setCopied] = useState(false);

  const handleCopyAll = () => {
    navigator.clipboard.writeText(message.content);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  return (
    <div className={clsx("flex gap-2.5 group", isUser ? "flex-row-reverse" : "flex-row")}>
      {/* Avatar */}
      <div
        className="w-8 h-8 rounded-lg flex items-center justify-center shrink-0 text-xs font-bold select-none shadow-sm"
        style={{
          background: isUser
            ? "linear-gradient(135deg, #43e97b 0%, #38f9d7 100%)"
            : "linear-gradient(135deg, #667eea 0%, #764ba2 100%)",
          color: "#fff",
        }}
      >
        {isUser ? "我" : "蟹"}
      </div>

      {/* Content */}
      <div className={clsx("max-w-[75%] min-w-0", isUser ? "items-end" : "items-start")}>
        <div
          className={clsx("relative rounded-xl px-3.5 py-2.5 text-[13px] leading-[1.7] break-words shadow-sm")}
          style={{
            background: isUser ? "#95ec69" : "var(--assistant-bubble)",
            color: isUser ? "#1a1a1a" : "var(--text-primary)",
          }}
        >
          {/* Notch */}
          <svg
            className={clsx("absolute top-3", isUser ? "-right-[5px]" : "-left-[5px]")}
            width="6" height="10" viewBox="0 0 6 10"
          >
            <path
              d={isUser ? "M0 0 Q6 5 0 10 Z" : "M6 0 Q0 5 6 10 Z"}
              fill={isUser ? "#95ec69" : "var(--assistant-bubble)"}
            />
          </svg>

          {message.isStreaming && !message.content ? (
            <div className="flex items-center gap-1.5" style={{ color: "var(--text-muted)" }}>
              <Loader2 size={13} className="animate-spin" />
              <span className="text-xs">思考中...</span>
            </div>
          ) : isUser ? (
            <span className="whitespace-pre-wrap">{message.content}</span>
          ) : (
            <div className="chat-markdown">
              <ReactMarkdown
                components={{
                  code({ className, children, ...props }) {
                    const match = /language-(\w+)/.exec(className || "");
                    const codeStr = String(children).replace(/\n$/, "");
                    if (match || codeStr.includes("\n")) {
                      return <CodeBlock language={match?.[1] || ""} value={codeStr} />;
                    }
                    return (
                      <code
                        className="inline-code"
                        style={{
                          background: "rgba(0,0,0,0.08)",
                          padding: "1px 5px",
                          borderRadius: "3px",
                          fontSize: "0.9em",
                          fontFamily: "'Cascadia Code', 'Fira Code', Consolas, monospace",
                        }}
                        {...props}
                      >
                        {children}
                      </code>
                    );
                  },
                  pre({ children }) {
                    return <>{children}</>;
                  },
                  table({ children }) {
                    return (
                      <div className="overflow-x-auto my-2">
                        <table
                          className="text-xs w-full"
                          style={{ borderCollapse: "collapse" }}
                        >
                          {children}
                        </table>
                      </div>
                    );
                  },
                  th({ children }) {
                    return (
                      <th
                        className="text-left px-2 py-1.5 font-semibold text-xs"
                        style={{ borderBottom: "2px solid var(--border)", background: "rgba(0,0,0,0.03)" }}
                      >
                        {children}
                      </th>
                    );
                  },
                  td({ children }) {
                    return (
                      <td
                        className="px-2 py-1.5 text-xs"
                        style={{ borderBottom: "1px solid var(--border)" }}
                      >
                        {children}
                      </td>
                    );
                  },
                  a({ href, children }) {
                    return (
                      <a href={href} target="_blank" rel="noopener noreferrer" style={{ color: "var(--accent)", textDecoration: "underline" }}>
                        {children}
                      </a>
                    );
                  },
                  blockquote({ children }) {
                    return (
                      <blockquote
                        className="my-2 pl-3 text-xs italic"
                        style={{ borderLeft: "3px solid var(--accent)", color: "var(--text-secondary)" }}
                      >
                        {children}
                      </blockquote>
                    );
                  },
                }}
              >
                {message.content}
              </ReactMarkdown>
            </div>
          )}
        </div>

        {/* Meta row */}
        <div
          className={clsx(
            "flex items-center gap-2 mt-1 px-1 opacity-0 group-hover:opacity-100 transition-opacity",
            isUser ? "flex-row-reverse" : "flex-row",
          )}
        >
          {message.model && (
            <span className="text-[10px]" style={{ color: "var(--text-muted)" }}>
              {message.model}
            </span>
          )}
          {!message.isStreaming && message.content && (
            <button
              onClick={handleCopyAll}
              className="flex items-center gap-0.5 text-[10px] transition-colors"
              style={{ color: "var(--text-muted)" }}
              title="复制全部"
            >
              {copied ? <Check size={10} /> : <Copy size={10} />}
              {copied ? "已复制" : "复制"}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
