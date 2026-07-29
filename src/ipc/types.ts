/** Shapes shared across the IPC boundary. Credentials never appear here. */

export type ConnectionState =
  | "disconnected"
  | "connecting"
  | "connected"
  | "disconnecting"
  | "error";

export type ProxyMode = "system-proxy" | "proxy-only";

/** A node as the WebView is allowed to see it: no uuid, no keys. */
export interface NodeView {
  id: string;
  name: string;
  protocol: string;
  /** Masked, e.g. "de1.•••.example.com:443" */
  endpoint: string;
}

export interface ConnectionEvent {
  state: ConnectionState;
  nodeId: string | null;
  /** Present only when state === "error"; already credential-redacted in Rust. */
  message: string | null;
}

export interface ImportPreview {
  nodes: NodeView[];
  skipped: number;
}

export interface AppSnapshot {
  connection: ConnectionEvent;
  nodes: NodeView[];
  selectedNodeId: string | null;
  mode: ProxyMode;
  localPort: number;
}
