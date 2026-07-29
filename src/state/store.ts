import { create } from "zustand";
import type {
  AppSnapshot,
  ConnectionEvent,
  ImportPreview,
  NodeView,
  ProxyMode,
} from "../ipc/types";
import * as ipc from "../ipc/commands";
import { onConnectionState } from "../ipc/events";

interface Toast {
  id: number;
  kind: "info" | "error";
  text: string;
}

interface AppStore {
  ready: boolean;
  connection: ConnectionEvent;
  nodes: NodeView[];
  selectedNodeId: string | null;
  mode: ProxyMode;
  localPort: number;
  importPreview: ImportPreview | null;
  toasts: Toast[];

  init: () => Promise<void>;
  toast: (kind: Toast["kind"], text: string) => void;
  dismissToast: (id: number) => void;
  previewImport: () => Promise<void>;
  cancelImport: () => void;
  commitImport: () => Promise<void>;
  selectNode: (id: string) => Promise<void>;
  deleteNode: (id: string) => Promise<void>;
  setMode: (mode: ProxyMode) => Promise<void>;
  toggleConnection: () => Promise<void>;
}

let toastSeq = 0;

function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}

export const useAppStore = create<AppStore>((set, get) => ({
  ready: false,
  connection: { state: "disconnected", nodeId: null, message: null },
  nodes: [],
  selectedNodeId: null,
  mode: "system-proxy",
  localPort: 2080,
  importPreview: null,
  toasts: [],

  init: async () => {
    await onConnectionState((connection) => {
      set({ connection });
      if (connection.state === "error" && connection.message) {
        get().toast("error", connection.message);
      }
    });
    const snap: AppSnapshot = await ipc.getSnapshot();
    set({
      ready: true,
      connection: snap.connection,
      nodes: snap.nodes,
      selectedNodeId: snap.selectedNodeId,
      mode: snap.mode,
      localPort: snap.localPort,
    });
  },

  toast: (kind, text) => {
    const id = ++toastSeq;
    set((s) => ({ toasts: [...s.toasts, { id, kind, text }] }));
    setTimeout(() => get().dismissToast(id), 4000);
  },

  dismissToast: (id) =>
    set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),

  previewImport: async () => {
    try {
      const preview = await ipc.previewClipboardImport();
      if (preview.nodes.length === 0) {
        get().toast(
          "error",
          preview.skipped > 0
            ? "Clipboard has links, but none could be read."
            : "No server link found in the clipboard.",
        );
        return;
      }
      set({ importPreview: preview });
    } catch (e) {
      get().toast("error", errorText(e));
    }
  },

  cancelImport: () => set({ importPreview: null }),

  commitImport: async () => {
    try {
      const nodes = await ipc.commitClipboardImport();
      const first = nodes[0];
      set((s) => ({
        importPreview: null,
        nodes,
        selectedNodeId: s.selectedNodeId ?? (first ? first.id : null),
      }));
      get().toast("info", "Server imported.");
    } catch (e) {
      get().toast("error", errorText(e));
    }
  },

  selectNode: async (id) => {
    await ipc.selectNode(id);
    set({ selectedNodeId: id });
  },

  deleteNode: async (id) => {
    await ipc.deleteNode(id);
    set((s) => ({
      nodes: s.nodes.filter((n) => n.id !== id),
      selectedNodeId: s.selectedNodeId === id ? null : s.selectedNodeId,
    }));
  },

  setMode: async (mode) => {
    try {
      await ipc.setMode(mode);
      set({ mode });
    } catch (e) {
      get().toast("error", errorText(e));
    }
  },

  toggleConnection: async () => {
    const { connection } = get();
    try {
      if (connection.state === "disconnected" || connection.state === "error") {
        await ipc.connect();
      } else if (connection.state === "connected") {
        await ipc.disconnect();
      }
    } catch (e) {
      get().toast("error", errorText(e));
    }
  },
}));
