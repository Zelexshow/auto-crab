import { create } from "zustand";

export interface Message {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  timestamp: number;
  model?: string;
  isStreaming?: boolean;
}

interface ChatState {
  messages: Message[];
  isLoading: boolean;
  currentStreamId: string | null;
  addMessage: (msg: Omit<Message, "id" | "timestamp">) => string;
  updateMessage: (id: string, content: string) => void;
  appendToMessage: (id: string, delta: string) => void;
  setMessageDone: (id: string, model?: string) => void;
  setLoading: (loading: boolean) => void;
  setStreamId: (id: string | null) => void;
  clearMessages: () => void;
}

let _counter = 0;
const genId = () => `msg-${Date.now()}-${++_counter}`;

export const useChatStore = create<ChatState>((set) => ({
  messages: [],
  isLoading: false,
  currentStreamId: null,

  addMessage: (msg) => {
    const id = genId();
    set((s) => ({
      messages: [...s.messages, { ...msg, id, timestamp: Date.now() }],
    }));
    return id;
  },

  updateMessage: (id, content) =>
    set((s) => ({
      messages: s.messages.map((m) =>
        m.id === id ? { ...m, content } : m,
      ),
    })),

  appendToMessage: (id, delta) =>
    set((s) => ({
      messages: s.messages.map((m) =>
        m.id === id ? { ...m, content: m.content + delta } : m,
      ),
    })),

  setMessageDone: (id, model) =>
    set((s) => ({
      messages: s.messages.map((m) =>
        m.id === id ? { ...m, isStreaming: false, model } : m,
      ),
    })),

  setLoading: (loading) => set({ isLoading: loading }),
  setStreamId: (id) => set({ currentStreamId: id }),
  clearMessages: () => set({ messages: [] }),
}));
