import { create } from "zustand";
import type {
  AppSnapshot,
  ConnectionEvent,
  ImportPreview,
  LogLine,
  NodeView,
  Settings,
  SubscriptionView,
  TestProgress,
  TrafficEvent,
  UpdateInterval,
} from "../ipc/types";
import * as ipc from "../ipc/commands";
import { interpolate, t } from "../i18n";
import {
  onConnectionState,
  onLatencyResult,
  onLogBatch,
  onSubsChanged,
  onTestProgress,
  onTraffic,
} from "../ipc/events";

export type Screen = "home" | "servers" | "subscriptions" | "logs" | "settings";

interface Toast {
  id: number;
  kind: "info" | "error";
  text: string;
}

interface AppStore {
  ready: boolean;
  screen: Screen;
  connection: ConnectionEvent;
  manualNodes: NodeView[];
  subscriptions: SubscriptionView[];
  selectedNodeId: string | null;
  settings: Settings;
  version: string;
  elevated: boolean;
  traffic: TrafficEvent | null;
  testing: TestProgress;
  importPreview: ImportPreview | null;
  toasts: Toast[];
  logLines: LogLine[];

  init: () => Promise<void>;
  refresh: () => Promise<void>;
  setScreen: (screen: Screen) => void;
  toast: (kind: Toast["kind"], text: string) => void;
  dismissToast: (id: number) => void;

  previewImport: () => Promise<void>;
  cancelImport: () => void;
  commitImport: () => Promise<void>;

  addSubscription: (name: string, url: string) => Promise<boolean>;
  updateSubscription: (id: string) => Promise<void>;
  deleteSubscription: (id: string) => Promise<void>;
  setSubAutoUpdate: (id: string, interval: UpdateInterval) => Promise<void>;

  selectNode: (id: string) => Promise<void>;
  deleteNode: (id: string) => Promise<void>;
  toggleFavorite: (id: string) => Promise<void>;
  copyNodeLink: (id: string) => Promise<void>;

  testAll: () => Promise<void>;
  testOne: (id: string) => Promise<void>;
  cancelTest: () => Promise<void>;

  saveSettings: (settings: Settings) => Promise<void>;
  toggleConnection: () => Promise<void>;

  loadLogs: () => Promise<void>;
  clearLogs: () => Promise<void>;
  copyLogs: () => Promise<void>;
  relaunchElevated: () => Promise<void>;
}

let toastSeq = 0;
let settingsRevision = 0;
let settingsSaveQueue: Promise<void> = Promise.resolve();

function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}

const defaultSettings: Settings = {
  mode: "system-proxy",
  localPort: 12334,
  allowLan: false,
  logLevel: "warn",
  autostart: false,
  startMinimized: false,
  autoConnect: false,
  rulesEnabled: true,
  adBlock: true,
  rulesetAutoUpdate: false,
  language: "en",
};

function patchLatency(
  nodes: NodeView[],
  nodeId: string,
  latencyMs: number | null,
): NodeView[] {
  return nodes.map((n) => (n.id === nodeId ? { ...n, latencyMs } : n));
}

export const useAppStore = create<AppStore>((set, get) => ({
  ready: false,
  screen: "home",
  connection: { state: "disconnected", nodeId: null, message: null },
  manualNodes: [],
  subscriptions: [],
  selectedNodeId: null,
  settings: defaultSettings,
  version: "",
  elevated: false,
  traffic: null,
  testing: { running: false, done: 0, total: 0 },
  importPreview: null,
  toasts: [],
  logLines: [],

  init: async () => {
    await onConnectionState((connection) => {
      set({ connection });
      if (connection.state !== "connected") {
        set({ traffic: null });
      }
      if (connection.state === "error" && connection.message) {
        get().toast("error", connection.message);
      }
    });
    await onTraffic((traffic) => set({ traffic }));
    await onLatencyResult(({ nodeId, latencyMs }) => {
      set((s) => ({
        manualNodes: patchLatency(s.manualNodes, nodeId, latencyMs),
        subscriptions: s.subscriptions.map((sub) => ({
          ...sub,
          nodes: patchLatency(sub.nodes, nodeId, latencyMs),
        })),
      }));
    });
    await onTestProgress((testing) => set({ testing }));
    await onSubsChanged(() => void get().refresh());
    await onLogBatch((batch) => {
      set((s) => {
        const merged = [...s.logLines, ...batch];
        return { logLines: merged.length > 2000 ? merged.slice(-2000) : merged };
      });
    });
    await get().refresh();
    set({ ready: true });
  },

  refresh: async () => {
    const snap: AppSnapshot = await ipc.getSnapshot();
    set({
      connection: snap.connection,
      manualNodes: snap.manualNodes,
      subscriptions: snap.subscriptions,
      selectedNodeId: snap.selectedNodeId,
      settings: snap.settings,
      version: snap.version,
      elevated: snap.elevated,
    });
  },

  setScreen: (screen) => set({ screen }),

  toast: (kind, text) => {
    const id = ++toastSeq;
    set((s) => ({ toasts: [...s.toasts, { id, kind, text }] }));
    setTimeout(() => get().dismissToast(id), 4500);
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
            ? preview.error ?? t("toastClipboardUnreadable")
            : t("toastClipboardNoLink"),
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
      await ipc.commitClipboardImport();
      set({ importPreview: null });
      await get().refresh();
      get().toast("info", t("toastImported"));
    } catch (e) {
      get().toast("error", errorText(e));
    }
  },

  addSubscription: async (name, url) => {
    try {
      await ipc.addSubscription(name, url);
      await get().refresh();
      get().toast("info", t("toastSubAdded"));
      return true;
    } catch (e) {
      await get().refresh(); // sub may exist with lastError set
      get().toast("error", errorText(e));
      return false;
    }
  },

  updateSubscription: async (id) => {
    try {
      const count = await ipc.updateSubscription(id);
      get().toast("info", interpolate(t("toastUpdatedN"), count));
    } catch (e) {
      get().toast("error", errorText(e));
    }
    await get().refresh();
  },

  deleteSubscription: async (id) => {
    try {
      await ipc.deleteSubscription(id);
      await get().refresh();
    } catch (e) {
      get().toast("error", errorText(e));
    }
  },

  setSubAutoUpdate: async (id, interval) => {
    try {
      await ipc.setSubAutoUpdate(id, interval);
      await get().refresh();
    } catch (e) {
      get().toast("error", errorText(e));
    }
  },

  selectNode: async (id) => {
    try {
      await ipc.selectNode(id);
      set({ selectedNodeId: id });
    } catch (e) {
      get().toast("error", errorText(e));
    }
  },

  deleteNode: async (id) => {
    try {
      await ipc.deleteNode(id);
      await get().refresh();
    } catch (e) {
      get().toast("error", errorText(e));
    }
  },

  toggleFavorite: async (id) => {
    try {
      await ipc.toggleFavorite(id);
      await get().refresh();
    } catch (e) {
      get().toast("error", errorText(e));
    }
  },

  copyNodeLink: async (id) => {
    try {
      await ipc.copyNodeLink(id);
      get().toast("info", t("toastLinkCopied"));
    } catch (e) {
      get().toast("error", errorText(e));
    }
  },

  testAll: async () => {
    try {
      await ipc.testNodes(null);
    } catch (e) {
      get().toast("error", errorText(e));
    }
  },

  testOne: async (id) => {
    try {
      await ipc.testNodes([id]);
    } catch (e) {
      get().toast("error", errorText(e));
    }
  },

  cancelTest: async () => {
    await ipc.cancelTest();
  },

  saveSettings: async (settings) => {
    const previous = get().settings;
    const revision = ++settingsRevision;
    set({ settings });
    const write = settingsSaveQueue.then(() => ipc.setSettings(settings));
    // Keep later writes moving even if one invocation fails.
    settingsSaveQueue = write.catch(() => undefined);
    try {
      await write;
      get().toast("info", t("toastSettingsSaved"));
    } catch (e) {
      if (revision === settingsRevision) set({ settings: previous });
      get().toast("error", errorText(e));
    }
  },

  loadLogs: async () => {
    try {
      set({ logLines: await ipc.getLogs() });
    } catch (e) {
      get().toast("error", errorText(e));
    }
  },

  clearLogs: async () => {
    await ipc.clearLogs();
    set({ logLines: [] });
  },

  copyLogs: async () => {
    try {
      await ipc.copyLogs();
      get().toast("info", t("toastLogsCopied"));
    } catch (e) {
      get().toast("error", errorText(e));
    }
  },

  relaunchElevated: async () => {
    try {
      await ipc.relaunchElevated();
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

/** All nodes across manual + subscriptions, for lookups. */
export function findNode(
  store: Pick<AppStore, "manualNodes" | "subscriptions">,
  id: string | null,
): NodeView | null {
  if (!id) return null;
  const manual = store.manualNodes.find((n) => n.id === id);
  if (manual) return manual;
  for (const sub of store.subscriptions) {
    const hit = sub.nodes.find((n) => n.id === id);
    if (hit) return hit;
  }
  return null;
}
