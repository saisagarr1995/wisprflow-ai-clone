import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

/** Standalone popover window for "ask local AI" / "explain this" answers. */
export default function Assistant() {
  const [answer, setAnswer] = useState("Thinking…");

  useEffect(() => {
    let unsub: (() => void) | undefined;
    listen<string>("assistant", (e) => setAnswer(e.payload)).then((u) => (unsub = u));
    return () => unsub?.();
  }, []);

  return (
    <div className="flex h-screen flex-col bg-neutral-950 text-neutral-100">
      <div className="flex items-center justify-between border-b border-neutral-800 px-4 py-2">
        <span className="text-sm font-semibold">Local AI</span>
        <button
          className="text-xs text-neutral-500 hover:text-neutral-200"
          onClick={() => navigator.clipboard.writeText(answer)}
        >
          Copy
        </button>
      </div>
      <div className="flex-1 overflow-y-auto whitespace-pre-wrap px-4 py-3 text-sm leading-relaxed text-neutral-200">
        {answer}
      </div>
    </div>
  );
}
