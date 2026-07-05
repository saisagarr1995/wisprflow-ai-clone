import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { PageHeader, inputCls } from "../ui";

interface HistoryEntry {
  id: number;
  raw: string;
  formatted: string;
  app: string | null;
  created_at: number;
  total_ms: number;
  stt_ms: number;
  llm_ms: number;
  chars: number;
}

export default function History() {
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [search, setSearch] = useState("");
  const [expanded, setExpanded] = useState<number | null>(null);

  const refresh = (q = search) =>
    invoke<HistoryEntry[]>("history_list", { search: q || null, limit: 100 })
      .then(setEntries)
      .catch(console.error);

  useEffect(() => {
    refresh();
    // Refresh when a new dictation lands.
    const timer = setInterval(() => refresh(), 5000);
    return () => clearInterval(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div>
      <div className="flex items-start justify-between">
        <PageHeader title="History" subtitle="Every dictation, stored locally. Click one to see the raw transcript and timings." />
        <button
          className="mt-1 text-xs text-neutral-500 transition-colors hover:text-red-400"
          onClick={() => invoke("history_clear").then(() => refresh())}
        >
          Clear all
        </button>
      </div>
      <input
        className={inputCls + " mb-4 w-full"}
        placeholder="Search dictations…"
        value={search}
        onChange={(e) => {
          setSearch(e.target.value);
          refresh(e.target.value);
        }}
      />

      <div className="flex flex-col gap-2">
        {entries.length === 0 && (
          <div className="py-12 text-center text-sm text-neutral-500">No dictations yet.</div>
        )}
        {entries.map((e) => (
          <div
            key={e.id}
            className="cursor-pointer rounded-xl border border-neutral-800 bg-neutral-900/60 px-4 py-3"
            onClick={() => setExpanded(expanded === e.id ? null : e.id)}
          >
            <div className="flex items-center justify-between gap-4">
              <div className="min-w-0 truncate text-sm text-neutral-200">{e.formatted}</div>
              <div className="flex shrink-0 items-center gap-3 text-[11px] text-neutral-500">
                <span>{new Date(e.created_at * 1000).toLocaleTimeString()}</span>
                <span>{e.total_ms} ms</span>
                <button
                  className="text-neutral-500 hover:text-neutral-200"
                  onClick={(ev) => {
                    ev.stopPropagation();
                    navigator.clipboard.writeText(e.formatted);
                  }}
                >
                  Copy
                </button>
                <button
                  className="text-neutral-500 hover:text-red-400"
                  onClick={(ev) => {
                    ev.stopPropagation();
                    invoke("history_delete", { id: e.id }).then(() => refresh());
                  }}
                >
                  ✕
                </button>
              </div>
            </div>
            {expanded === e.id && (
              <div className="mt-3 border-t border-neutral-800 pt-3 text-xs">
                <div className="mb-1 text-neutral-500">Raw transcript:</div>
                <div className="mb-2 text-neutral-400">{e.raw}</div>
                <div className="text-neutral-600">
                  STT {e.stt_ms} ms · LLM {e.llm_ms} ms · total {e.total_ms} ms · {e.chars} chars
                </div>
              </div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
