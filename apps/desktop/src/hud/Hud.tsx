import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";

const BAR_COUNT = 36;

/**
 * The floating dictation capsule: a cream pill with a live waveform, a state
 * pill above it, and the streaming partial transcript below. The window is
 * transparent, click-through, and never focused.
 */
export default function Hud() {
  const [status, setStatus] = useState<"recording" | "processing" | "idle">("idle");
  const [partial, setPartial] = useState("");
  const [bars, setBars] = useState<number[]>(() => Array(BAR_COUNT).fill(0.04));
  const levelsRef = useRef<number[]>(Array(BAR_COUNT).fill(0.04));

  useEffect(() => {
    const unsubs: Array<() => void> = [];
    listen<string>("status", (e) => {
      setStatus(e.payload as "recording" | "processing" | "idle");
      if (e.payload === "recording") {
        setPartial("");
        levelsRef.current = Array(BAR_COUNT).fill(0.04);
        setBars([...levelsRef.current]);
      }
    }).then((u) => unsubs.push(u));
    listen<number>("level", (e) => {
      const next = [...levelsRef.current.slice(1), Math.min(1, e.payload * 6 + 0.04)];
      levelsRef.current = next;
      setBars(next);
    }).then((u) => unsubs.push(u));
    listen<string>("partial", (e) => setPartial(e.payload)).then((u) => unsubs.push(u));
    return () => unsubs.forEach((u) => u());
  }, []);

  const statePill =
    status === "recording"
      ? { text: "● Listening", cls: "bg-emerald-800 text-emerald-50" }
      : { text: "Thinking…", cls: "bg-amber-600 text-amber-50" };

  return (
    <div className="flex h-screen w-screen select-none flex-col items-center justify-end gap-2 bg-transparent pb-2">
      <div className={`rounded-full px-4 py-1 text-sm font-semibold shadow-lg ${statePill.cls}`}>
        {statePill.text}
      </div>

      <div className="flex h-16 items-center gap-[3px] rounded-full border-[3px] border-neutral-900 bg-[#f6f1e7] px-7 shadow-xl">
        {bars.map((v, i) => (
          <div
            key={i}
            className={`w-[3.5px] rounded-full bg-neutral-900 ${
              status === "processing" ? "animate-pulse" : ""
            }`}
            style={{ height: `${Math.round(6 + v * 40)}px` }}
          />
        ))}
      </div>

      {partial && (
        <div className="max-w-[400px] truncate rounded-full bg-neutral-900/85 px-4 py-1 text-sm text-neutral-100 shadow-lg">
          {partial}
        </div>
      )}
    </div>
  );
}
