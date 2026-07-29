import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ConnectionEvent,
  LatencyResult,
  TestProgress,
  TrafficEvent,
} from "./types";

export const onConnectionState = (
  handler: (event: ConnectionEvent) => void,
): Promise<UnlistenFn> =>
  listen<ConnectionEvent>("connection-state", (e) => handler(e.payload));

export const onTraffic = (
  handler: (event: TrafficEvent) => void,
): Promise<UnlistenFn> => listen<TrafficEvent>("traffic", (e) => handler(e.payload));

export const onLatencyResult = (
  handler: (event: LatencyResult) => void,
): Promise<UnlistenFn> =>
  listen<LatencyResult>("latency-result", (e) => handler(e.payload));

export const onTestProgress = (
  handler: (event: TestProgress) => void,
): Promise<UnlistenFn> =>
  listen<TestProgress>("test-progress", (e) => handler(e.payload));

export const onSubsChanged = (handler: () => void): Promise<UnlistenFn> =>
  listen("subs-changed", () => handler());

export const onLogBatch = (
  handler: (lines: import("./types").LogLine[]) => void,
): Promise<UnlistenFn> =>
  listen<import("./types").LogLine[]>("log-batch", (e) => handler(e.payload));
