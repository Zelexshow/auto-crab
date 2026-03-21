import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

export interface Message {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  timestamp: number;
  model?: string;
  isStreaming?: boolean;
}

export interface ConversationSummary {
  id: string;
  title: string;
  updated_at: string;
  message_count: number;
}

interface ChatState {
  conversationId: string | null;
  conversationTitle: string;
  messages: Message[];
  conversations: ConversationSummary[];
  isLoading: boolean;
  currentStreamId: string | null;

  addMessage: (msg: Omit<Message, "id" | "timestamp">) => string;
  updateMessage: (id: string, content: string) => void;
  appendToMessage: (id: string, delta: string) => void;
  setMessageDone: (id: string, model?: string) => void;
  setLoading: (loading: boolean) => void;
  setStreamId: (id: string | null) => void;

  newConversation: () => void;
  saveCurrentConversation: () => Promise<void>;
  loadConversation: (id: string) => Promise<void>;
  deleteConversation: (id: string) => Promise<void>;
  renameConversation: (id: string, newTitle: string) => Promise<void>;
  refreshConversationList: () => Promise<void>;
}

let _counter = 0;
const genId = () => `msg-${Date.now()}-${++_counter}`;
const genConvId = () => `conv-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;

export const useChatStore = create<ChatState>((set, get) => ({
  conversationId: null,
  conversationTitle: "新对话",
  messages: [],
  conversations: [],
  isLoading: false,
  currentStreamId: null,

  addMessage: (msg) => {
    const id = genId();
    set((s) => ({
      messages: [...s.messages, { ...msg, id, timestamp: Date.now() }],
    }));
    // Auto-set title from first user message
    const state = get();
    if (msg.role === "user" && state.messages.length <= 1) {
      const title = msg.content.slice(0, 30) + (msg.content.length > 30 ? "..." : "");
      set({ conversationTitle: title });
    }
    return id;
  },

  updateMessage: (id, content) =>
    set((s) => ({
      messages: s.messages.map((m) => (m.id === id ? { ...m, content } : m)),
    })),

  appendToMessage: (id, delta) =>
    set((s) => ({
      messages: s.messages.map((m) => (m.id === id ? { ...m, content: m.content + delta } : m)),
    })),

  setMessageDone: (id, model) => {
    set((s) => ({
      messages: s.messages.map((m) => (m.id === id ? { ...m, isStreaming: false, model } : m)),
    }));
    // Auto-save after assistant responds
    get().saveCurrentConversation();
  },

  setLoading: (loading) => set({ isLoading: loading }),
  setStreamId: (id) => set({ currentStreamId: id }),

  newConversation: () => {
    set({
      conversationId: null,
      conversationTitle: "新对话",
      messages: [],
    });
  },

  saveCurrentConversation: async () => {
    const state = get();
    if (state.messages.length === 0) return;

    const convId = state.conversationId || genConvId();
    if (!state.conversationId) {
      set({ conversationId: convId });
    }

    const storedMessages = state.messages
      .filter((m) => !m.isStreaming)
      .map((m) => ({
        role: m.role,
        content: m.content,
        timestamp: new Date(m.timestamp).toISOString(),
        model: m.model || null,
      }));

    try {
      await invoke("save_conversation", {
        conversation: {
          id: convId,
          title: state.conversationTitle,
          created_at: new Date(state.messages[0]?.timestamp || Date.now()).toISOString(),
          updated_at: new Date().toISOString(),
          messages: storedMessages,
        },
      });
      get().refreshConversationList();
    } catch (e) {
      console.error("Failed to save conversation:", e);
    }
  },

  loadConversation: async (id: string) => {
    try {
      const result = await invoke<{
        success: boolean;
        data?: {
          id: string;
          title: string;
          messages: { role: string; content: string; timestamp: string; model?: string }[];
        };
        error?: string;
      }>("load_conversation", { id });

      if (result.success && result.data) {
        const msgs: Message[] = result.data.messages.map((m, i) => ({
          id: `loaded-${i}-${Date.now()}`,
          role: m.role as Message["role"],
          content: m.content,
          timestamp: new Date(m.timestamp).getTime(),
          model: m.model || undefined,
        }));
        set({
          conversationId: result.data.id,
          conversationTitle: result.data.title,
          messages: msgs,
        });
      }
    } catch (e) {
      console.error("Failed to load conversation:", e);
    }
  },

  deleteConversation: async (id: string) => {
    try {
      await invoke("delete_conversation", { id });
      const state = get();
      if (state.conversationId === id) {
        set({ conversationId: null, conversationTitle: "新对话", messages: [] });
      }
      get().refreshConversationList();
    } catch (e) {
      console.error("Failed to delete conversation:", e);
    }
  },

  renameConversation: async (id: string, newTitle: string) => {
    try {
      const result = await invoke<{
        success: boolean;
        data?: {
          id: string;
          title: string;
          messages: { role: string; content: string; timestamp: string; model?: string }[];
          created_at: string;
          updated_at: string;
        };
      }>("load_conversation", { id });

      if (result.success && result.data) {
        result.data.title = newTitle;
        result.data.updated_at = new Date().toISOString();
        await invoke("save_conversation", { conversation: result.data });

        const state = get();
        if (state.conversationId === id) {
          set({ conversationTitle: newTitle });
        }
        get().refreshConversationList();
      }
    } catch (e) {
      console.error("Failed to rename conversation:", e);
    }
  },

  refreshConversationList: async () => {
    try {
      const result = await invoke<{
        success: boolean;
        data?: ConversationSummary[];
      }>("list_conversations");
      if (result.success && result.data) {
        set({ conversations: result.data });
      }
    } catch (e) {
      console.error("Failed to list conversations:", e);
    }
  },
}));
