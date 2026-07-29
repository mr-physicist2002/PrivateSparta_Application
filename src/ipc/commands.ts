import { invoke } from "@tauri-apps/api/core";
import type { AppSnapshot, ImportPreview, NodeView, ProxyMode } from "./types";

export function getSnapshot(): Promise<AppSnapshot> {
  return invoke<AppSnapshot>("get_snapshot");
}

/** Reads the clipboard in Rust and returns a preview; nothing is stored yet. */
export function previewClipboardImport(): Promise<ImportPreview> {
  return invoke<ImportPreview>("preview_clipboard_import");
}

/** Commits the nodes from the last preview to the store. */
export function commitClipboardImport(): Promise<NodeView[]> {
  return invoke<NodeView[]>("commit_clipboard_import");
}

export function selectNode(nodeId: string): Promise<void> {
  return invoke("select_node", { nodeId });
}

export function connect(): Promise<void> {
  return invoke("connect");
}

export function disconnect(): Promise<void> {
  return invoke("disconnect");
}

export function setMode(mode: ProxyMode): Promise<void> {
  return invoke("set_mode", { mode });
}

export function deleteNode(nodeId: string): Promise<void> {
  return invoke("delete_node", { nodeId });
}
