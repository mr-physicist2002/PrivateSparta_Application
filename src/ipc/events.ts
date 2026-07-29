import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ConnectionEvent } from "./types";

export function onConnectionState(
  handler: (event: ConnectionEvent) => void,
): Promise<UnlistenFn> {
  return listen<ConnectionEvent>("connection-state", (e) => handler(e.payload));
}
