import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

interface Timing {
  audio_ms: number;
  stt_ms: number;
  llm_ms: number;
  inject_ms: number;
  total_ms: number;
  chars: number;
}

const STAGES: Array<[keyof Timing, string, string]> = [
  ["stt_ms", "Transcribe", "bg-indigo-500"],
  ["llm_ms", "AI cleanup", "bg-emerald-500"],
  ["inject_ms", "Insert", "bg-amber-500"],
];

function percentile(values: number[], p: number): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.floor((p / 100) * sorted.length))];
}

export default function Latency() {
  const [timings, setTimings] = useState<Timing[]>([]);
  const buf = useRef<Timing[]>([]);

  useEffect(() => {
    let unsub: (() => void) | undefined;
    listen<Timing>("timing", (e) => {
      buf.current = [...buf.current, e.payload].slice(-50);
      setTimings([...buf.current]);
    }).then((u) => (unsub = u));
    return () => unsub?.();
  }, []);

  const totals = timings.map((t) => t.total_ms);
  const p50 = percentile(totals, 50);
  const p95 = percentile(totals, 95);
  const maxTotal = Math.max(1, ...totals);

  return (
    <div className="max-w-3xl">
      <h1 className="mb-2 text-xl font-semibold">Latency</h1>
      <p className="mb-6 text-sm text-neutral-500">
        End-to-end time from releasing the hotkey to text appearing, for your last{" "}
        {timings.length} dictations. Target: under 500 ms typical.
      </p>

      <div className="mb-8 grid grid-cols-3 gap-4">
        {[
          ["p50", p50],
          ["p95", p95],
          ["samples", timings.length],
        ].map(([label, val]) => (
          <div key={label} className="rounded-xl border border-neutral-800 bg-neutral-900/60 p-4">
            <div className="text-xs uppercase tracking-wider text-neutral-500">{label}</div>
            <div className="mt-1 text-2xl font-semibold text-neutral-100">
              {val}
              {label !== "samples" && <span className="text-sm text-neutral-500"> ms</span>}
            </div>
          </div>
        ))}
      </div>

      {timings.length === 0 ? (
        <div className="py-12 text-center text-sm text-neutral-500">
          Dictate something — timings appear here live.
        </div>
      ) : (
        <div className="flex flex-col gap-1.5">
          {[...timings].reverse().map((t, i) => (
            <div key={i} className="flex items-center gap-3">
              <div className="flex h-5 flex-1 overflow-hidden rounded bg-neutral-900">
                {STAGES.map(([key, , color]) => (
                  <div
                    key={key}
                    className={color}
                    style={{ width: `${(Number(t[key]) / maxTotal) * 100}%` }}
                    title={`${key}: ${t[key]} ms`}
                  />
                ))}
              </div>
              <div className="w-16 text-right text-xs tabular-nums text-neutral-400">
                {t.total_ms} ms
              </div>
            </div>
          ))}
        </div>
      )}

      <div className="mt-6 flex gap-4 text-xs text-neutral-500">
        {STAGES.map(([key, label, color]) => (
          <div key={key} className="flex items-center gap-1.5">
            <span className={`inline-block h-2.5 w-2.5 rounded-sm ${color}`} />
            {label}
          </div>
        ))}
      </div>
    </div>
  );
}
