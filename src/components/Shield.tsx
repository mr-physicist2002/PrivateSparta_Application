import { useEffect, useRef, useState } from "react";
import type { ConnectionState } from "../ipc/types";

const LAMBDA_PATH = "M64 148 L100 52 L136 148 M84 148 L100 106";
const RIM_RADIUS = 74;

interface ShieldProps {
  state: ConnectionState;
  /** Current throughput in bytes/sec (up + down); drives the bloom. */
  throughputBps: number;
  disabled: boolean;
  onClick: () => void;
  label: string;
}

/**
 * The lambda shield — the one visually bold element in the app.
 * disconnected: hairline bronze stroke, empty interior.
 * connecting:   the stroke draws itself once, interior pulses bronze.
 * connected:    filled bronze; the bloom behind it breathes with traffic.
 * error:        crimson stroke, no bloom.
 */
export function Shield({ state, throughputBps, disabled, onClick, label }: ShieldProps) {
  const lambdaRef = useRef<SVGPathElement>(null);
  const [pathLength, setPathLength] = useState(400);
  useEffect(() => {
    if (lambdaRef.current) {
      setPathLength(Math.ceil(lambdaRef.current.getTotalLength()));
    }
  }, []);

  // 0 → 0.15, 5 MB/s and up → 0.45; the app breathes when data moves.
  const bloomOpacity = Math.min(0.45, 0.15 + (throughputBps / 5_000_000) * 0.3);

  const error = state === "error";
  const connected = state === "connected";
  const connecting = state === "connecting" || state === "disconnecting";
  const stroke = error ? "var(--color-danger)" : "var(--color-accent)";

  return (
    <button
      onClick={onClick}
      disabled={disabled}
      aria-label={label}
      className="group relative outline-none disabled:cursor-not-allowed"
      style={{ width: 200, height: 200 }}
    >
      <svg
        viewBox="0 0 200 200"
        width="200"
        height="200"
        style={{ ["--shield-path-length" as string]: pathLength }}
      >
        <defs>
          <radialGradient id="shield-bloom" cx="50%" cy="50%" r="50%">
            <stop offset="0%" stopColor="var(--color-accent)" stopOpacity="0.9" />
            <stop offset="60%" stopColor="var(--color-accent)" stopOpacity="0.25" />
            <stop offset="100%" stopColor="var(--color-accent)" stopOpacity="0" />
          </radialGradient>
        </defs>

        {/* Bloom — connected only, opacity driven by live throughput. */}
        <circle
          cx="100"
          cy="100"
          r="98"
          fill="url(#shield-bloom)"
          style={{
            opacity: connected ? bloomOpacity : 0,
            transition: "opacity 700ms cubic-bezier(.2,.8,.2,1)",
          }}
        />

        {/* Shield rim. */}
        <circle
          cx="100"
          cy="100"
          r={RIM_RADIUS}
          fill={connected ? "var(--color-accent)" : "transparent"}
          stroke={stroke}
          strokeWidth={connected ? 0 : 1.25}
          style={{ transition: "fill 300ms cubic-bezier(.2,.8,.2,1)" }}
        />

        {/* Connecting pulse fill. */}
        {connecting ? (
          <circle
            cx="100"
            cy="100"
            r={RIM_RADIUS - 1}
            fill="var(--color-accent)"
            className="shield-pulse"
          />
        ) : null}

        {/* The lambda. */}
        <path
          ref={lambdaRef}
          d={LAMBDA_PATH}
          fill="none"
          stroke={connected ? "var(--color-bg-base)" : stroke}
          strokeWidth="7"
          strokeLinecap="round"
          strokeLinejoin="round"
          className={connecting ? "shield-draw" : undefined}
          style={{ transition: "stroke 300ms cubic-bezier(.2,.8,.2,1)" }}
        />

        {/* Hover affordance when idle. */}
        {!connected && !connecting && !disabled ? (
          <circle
            cx="100"
            cy="100"
            r={RIM_RADIUS}
            fill="var(--color-accent-wash)"
            className="opacity-0 transition-opacity duration-[140ms] group-hover:opacity-100"
          />
        ) : null}
      </svg>
    </button>
  );
}
