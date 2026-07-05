import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useConfig } from "../lib/useConfig";
import Commands from "./pages/Commands";
import Dictionary from "./pages/Dictionary";
import General from "./pages/General";
import History from "./pages/History";
import Latency from "./pages/Latency";
import Microphone from "./pages/Microphone";
import Models from "./pages/Models";
import Snippets from "./pages/Snippets";

type SectionId =
  | "general"
  | "microphone"
  | "models"
  | "dictionary"
  | "snippets"
  | "commands"
  | "history"
  | "latency";

const NAV: Array<{ group: string; items: Array<{ id: SectionId; label: string; icon: string }> }> = [
  {
    group: "Setup",
    items: [
      { id: "general", label: "General", icon: "⚙" },
      { id: "microphone", label: "Microphone", icon: "🎙" },
      { id: "models", label: "Models & AI", icon: "✦" },
    ],
  },
  {
    group: "Personalize",
    items: [
      { id: "dictionary", label: "Dictionary", icon: "A" },
      { id: "snippets", label: "Snippets", icon: "⚡" },
      { id: "commands", label: "Commands", icon: "⌘" },
    ],
  },
  {
    group: "Insights",
    items: [
      { id: "history", label: "History", icon: "◷" },
      { id: "latency", label: "Latency", icon: "◍" },
    ],
  },
];

export default function Settings() {
  const [active, setActive] = useState<SectionId>("general");
  const [status, setStatus] = useState<"idle" | "recording" | "processing">("idle");
  const { config, update } = useConfig();

  useEffect(() => {
    let unsub: (() => void) | undefined;
    listen<string>("status", (e) => setStatus(e.payload as typeof status)).then(
      (u) => (unsub = u),
    );
    return () => unsub?.();
  }, []);

  if (!config) {
    return (
      <div className="flex h-screen items-center justify-center bg-neutral-950 text-neutral-500">
        Loading…
      </div>
    );
  }

  const statusMeta = {
    idle: { dot: "bg-emerald-400", label: "Ready" },
    recording: { dot: "bg-red-400 animate-pulse", label: "Listening" },
    processing: { dot: "bg-amber-400 animate-pulse", label: "Transcribing" },
  }[status];

  return (
    <div className="flex h-screen bg-neutral-950 text-neutral-100">
      <aside className="flex w-60 shrink-0 flex-col border-r border-white/5 bg-neutral-900/40 p-4">
        <div className="mb-6 flex items-center gap-2.5 px-1 pt-1">
          <div className="of-brand-gradient flex h-8 w-8 items-center justify-center rounded-xl shadow-lg shadow-indigo-900/40">
            <WaveMark />
          </div>
          <div className="leading-tight">
            <div className="text-[15px] font-semibold tracking-tight">OpenFlow</div>
            <div className="text-[10px] text-neutral-500">Local voice dictation</div>
          </div>
        </div>

        <nav className="flex flex-1 flex-col gap-5 overflow-y-auto">
          {NAV.map((section) => (
            <div key={section.group}>
              <div className="mb-1.5 px-2 text-[10px] font-semibold uppercase tracking-widest text-neutral-600">
                {section.group}
              </div>
              <div className="flex flex-col gap-0.5">
                {section.items.map((item) => (
                  <button
                    key={item.id}
                    onClick={() => setActive(item.id)}
                    className={`group flex items-center gap-2.5 rounded-lg px-2.5 py-2 text-left text-[13.5px] transition-colors ${
                      active === item.id
                        ? "bg-white/10 font-medium text-white"
                        : "text-neutral-400 hover:bg-white/5 hover:text-neutral-200"
                    }`}
                  >
                    <span
                      className={`flex h-5 w-5 items-center justify-center rounded text-[11px] ${
                        active === item.id ? "text-indigo-300" : "text-neutral-500"
                      }`}
                    >
                      {item.icon}
                    </span>
                    {item.label}
                  </button>
                ))}
              </div>
            </div>
          ))}
        </nav>

        <div className="mt-4 flex items-center gap-2 rounded-lg bg-white/5 px-3 py-2">
          <span className={`inline-block h-2 w-2 rounded-full ${statusMeta.dot}`} />
          <span className="text-xs text-neutral-400">{statusMeta.label}</span>
          <span className="ml-auto text-[10px] text-neutral-600">100% local</span>
        </div>
      </aside>

      <main className="flex-1 overflow-y-auto">
        <div className="mx-auto max-w-2xl px-10 py-9">
          {active === "general" && <General config={config} update={update} />}
          {active === "microphone" && <Microphone config={config} update={update} />}
          {active === "models" && <Models config={config} update={update} />}
          {active === "dictionary" && <Dictionary />}
          {active === "snippets" && <Snippets />}
          {active === "commands" && <Commands />}
          {active === "history" && <History />}
          {active === "latency" && <Latency />}
        </div>
      </main>
    </div>
  );
}

function WaveMark() {
  // Miniature of the app icon's waveform, in white.
  const heights = [7, 12, 17, 12, 8];
  return (
    <svg width="18" height="18" viewBox="0 0 18 18" aria-hidden>
      {heights.map((h, i) => (
        <rect
          key={i}
          x={2 + i * 3.2}
          y={(18 - h) / 2}
          width="2"
          height={h}
          rx="1"
          fill="white"
        />
      ))}
    </svg>
  );
}
