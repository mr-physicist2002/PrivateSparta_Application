import { invoke } from "@tauri-apps/api/core";
import type {
  AppSnapshot,
  ImportPreview,
  Settings,
  UpdateInterval,
} from "./types";

export const getSnapshot = () => invoke<AppSnapshot>("get_snapshot");

/** Reads the clipboard in Rust and returns a preview; nothing is stored yet. */
export const previewClipboardImport = () =>
  invoke<ImportPreview>("preview_clipboard_import");
export const commitClipboardImport = () => invoke<void>("commit_clipboard_import");

export const addSubscription = (name: string, url: string) =>
  invoke<void>("add_subscription", { name, url });
export const updateSubscription = (subId: string) =>
  invoke<number>("update_subscription", { subId });
export const deleteSubscription = (subId: string) =>
  invoke<void>("delete_subscription", { subId });
export const setSubAutoUpdate = (subId: string, interval: UpdateInterval) =>
  invoke<void>("set_sub_auto_update", { subId, interval });
export const revealSubscriptionUrl = (subId: string) =>
  invoke<string>("reveal_subscription_url", { subId });

export const selectNode = (nodeId: string) => invoke<void>("select_node", { nodeId });
export const deleteNode = (nodeId: string) => invoke<void>("delete_node", { nodeId });
export const toggleFavorite = (nodeId: string) =>
  invoke<boolean>("toggle_favorite", { nodeId });
/** Rebuilds the share URI Rust-side and copies it; the URI never reaches JS. */
export const copyNodeLink = (nodeId: string) =>
  invoke<void>("copy_node_link", { nodeId });

export const testNodes = (nodeIds: string[] | null) =>
  invoke<void>("test_nodes", { nodeIds });
export const cancelTest = () => invoke<void>("cancel_test");

export const setSettings = (settings: Settings) =>
  invoke<void>("set_settings", { settings });

export const connect = () => invoke<void>("connect");
export const disconnect = () => invoke<void>("disconnect");

export const getLogs = () => invoke<import("./types").LogLine[]>("get_logs");
export const clearLogs = () => invoke<void>("clear_logs");
/** Joins and copies in Rust; log text never round-trips through JS. */
export const copyLogs = () => invoke<void>("copy_logs");
export const relaunchElevated = () => invoke<void>("relaunch_elevated");

export interface UpdateInfo {
  available: boolean;
  version: string | null;
  notes: string | null;
}
export const checkForUpdate = () => invoke<UpdateInfo>("check_for_update");
export const installUpdate = () => invoke<void>("install_update");
