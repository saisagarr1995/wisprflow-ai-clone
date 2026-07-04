import { useState } from "react";
import { useConfig } from "../lib/useConfig";
import Commands from "./pages/Commands";
import Dictionary from "./pages/Dictionary";
import General from "./pages/General";
import History from "./pages/History";
import Latency from "./pages/Latency";
import Microphone from "./pages/Microphone";
import Models from "./pages/Models";
import Snippets from "./pages/Snippets";

const SECTIONS = [
  { id: "general", label: "General" },
  { id: "microphone", label: "Microphone" },
  { id: "models", label: "Models" },
  { id: "dictionary", label: "Dictionary" },
  { id: "snippets", label: "Snippets" },
  { id: "history", label: "History" },
  { id: "profiles", label: "Profiles" },
  { id: "commands", label: "Commands" },
  { id: "latency", label: "Latency" },
] as const;

type SectionId = (typeof SECTIONS)[number]["id"];

export default function Settings() {
  const [active, setActive] = useState<SectionId>("general");
  const { config, update } = useConfig();

  if (!config) {
    return (
      <div className="flex h-screen items-center justify-center bg-neutral-950 text-neutral-500">
        Loading…
      </div>
    );
  }

  return (
    <div className="flex h-screen bg-neutral-950 text-neutral-100">
      <aside className="flex w-52 shrink-0 flex-col border-r border-neutral-800 bg-neutral-900/40 p-3">
        <div className="mb-4 flex items-center gap-2 px-2 pt-1">
          <div className="flex h-7 w-7 items-center justify-center rounded-lg bg-indigo-600 text-sm">
            🎙
          </div>
          <span className="text-base font-semibold tracking-tight">OpenFlow</span>
        </div>
        <nav className="flex flex-col gap-0.5">
          {SECTIONS.map((s) => (
            <button
              key={s.id}
              onClick={() => setActive(s.id)}
              className={`rounded-lg px-3 py-1.5 text-left text-sm transition-colors ${
                active === s.id
                  ? "bg-neutral-800 font-medium text-white"
                  : "text-neutral-400 hover:bg-neutral-800/60 hover:text-neutral-200"
              }`}
            >
              {s.label}
            </button>
          ))}
        </nav>
        <div className="mt-auto px-2 pb-1 text-[11px] leading-relaxed text-neutral-600">
          100% local. No cloud, no telemetry.
        </div>
      </aside>

      <main className="flex-1 overflow-y-auto p-8">
        {active === "general" && <General config={config} update={update} />}
        {active === "microphone" && <Microphone config={config} update={update} />}
        {active === "models" && <Models config={config} update={update} />}
        {active === "dictionary" && <Dictionary />}
        {active === "snippets" && <Snippets />}
        {active === "history" && <History />}
        {active === "commands" && <Commands />}
        {active === "latency" && <Latency />}
        {active === "profiles" && (
          <div className="mt-24 text-center text-neutral-500">
            <div className="text-lg">Profiles</div>
            <div className="mt-1 text-sm">
              Per-app profiles are auto-applied today (chat, email, coding, terminal, document).
              A profile editor UI is on the roadmap.
            </div>
          </div>
        )}
      </main>
    </div>
  );
}
