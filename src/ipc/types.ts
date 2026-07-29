/** Shapes shared across the IPC boundary. Credentials never appear here. */

export type ConnectionState =
  | "disconnected"
  | "connecting"
  | "connected"
  | "disconnecting"
  | "error";

export type ProxyMode = "system-proxy" | "proxy-only";
export type UpdateInterval = "off" | "6h" | "12h" | "24h";

/** A node as the WebView is allowed to see it: no uuid, no keys. */
export interface NodeView {
  id: string;
  name: string;
  protocol: string;
  /** Masked, e.g. "de1.•••.example.com:443" */
  endpoint: string;
  latencyMs: number | null;
  favorite: boolean;
}

export interface SubUserInfo {
  upload: number;
  download: number;
  total: number;
  expire: number | null;
}

export interface SubscriptionView {
  id: string;
  name: string;
  urlMasked: string;
  nodeCount: number;
  userInfo: SubUserInfo | null;
  autoUpdate: UpdateInterval;
  lastUpdated: number | null;
  lastError: string | null;
  nodes: NodeView[];
}

export interface Settings {
  mode: ProxyMode;
  localPort: number;
  allowLan: boolean;
  logLevel: string;
  autostart: boolean;
  startMinimized: boolean;
  autoConnect: boolean;
}

export interface ConnectionEvent {
  state: ConnectionState;
  nodeId: string | null;
  /** Present only when state === "error"; already credential-redacted in Rust. */
  message: string | null;
}

export interface TrafficEvent {
  upBps: number;
  downBps: number;
  upTotal: number;
  downTotal: number;
  seconds: number;
}

export interface LatencyResult {
  nodeId: string;
  latencyMs: number | null;
}

export interface TestProgress {
  running: boolean;
  done: number;
  total: number;
}

export interface ImportPreview {
  nodes: NodeView[];
  skipped: number;
}

export interface AppSnapshot {
  connection: ConnectionEvent;
  manualNodes: NodeView[];
  subscriptions: SubscriptionView[];
  selectedNodeId: string | null;
  settings: Settings;
  version: string;
}
