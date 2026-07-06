import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import "./hud.css";

const log = (msg: string) => invoke("ui_log", { msg }).catch(() => {});

type HudState = "idle" | "recording" | "processing" | "flash";

const BAR_COUNT = 14;
const IDLE_BARS = Array(BAR_COUNT).fill(0.12);

/**
 * The Wispr Flow-style floating pill. Minimal and compact:
 *  - idle: a tiny handle bar at the very bottom of the screen
 *  - recording: a small dark capsule with a live waveform; a text pill with the
 *    streaming partial floats above it once words arrive
 *  - processing: the waveform gently pulses while the final pass + LLM run
 *  - flash: a brief confirmation pill after a voice command
 * The window is transparent and click-through; content is bottom-anchored.
 */
export default function Hud() {
  const [state, setState] = useState<HudState>("idle");
  const [partial, setPartial] = useState("");
  const [flash, setFlash] = useState("");
  const [bars, setBars] = useState<number[]>(IDLE_BARS);
  const levelsRef = useRef<number[]>([...IDLE_BARS]);
  const flashTimer = useRef<number | undefined>(undefined);

  useEffect(() => {
    const unsubs: Array<() => void> = [];
    log("hud mounted");

    listen<string>("status", (e) => {
      const s = e.payload as "idle" | "recording" | "processing";
      log(`hud status=${s}`);
      setState(s);
      if (s === "recording") {
        setPartial("");
        setFlash("");
        levelsRef.current = [...IDLE_BARS];
        setBars([...IDLE_BARS]);
      }
    }).then((u) => unsubs.push(u));

    listen<number>("level", (e) => {
      // Map RMS → bar height with headroom; scroll the ring left.
      const v = Math.min(1, Math.max(0.12, e.payload * 7));
      const next = [...levelsRef.current.slice(1), v];
      levelsRef.current = next;
      setBars(next);
    }).then((u) => unsubs.push(u));

    listen<string>("partial", (e) => setPartial(e.payload)).then((u) => unsubs.push(u));

    listen<string>("flash", (e) => {
      window.clearTimeout(flashTimer.current);
      setFlash(e.payload);
      setState("flash");
      flashTimer.current = window.setTimeout(() => {
        setFlash("");
        setState("idle");
      }, 2000);
    }).then((u) => unsubs.push(u));

    return () => {
      unsubs.forEach((u) => u());
      window.clearTimeout(flashTimer.current);
    };
  }, []);

  return (
    <div className="hud-root">
      {state === "idle" && <div className="hud-handle" />}

      {state === "flash" && (
        <div className="hud-flash">
          <svg viewBox="0 0 20 20" className="hud-flash-check" aria-hidden>
            <path
              d="M5 10.5l3 3 7-7"
              fill="none"
              stroke="currentColor"
              strokeWidth="2.2"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
          <span>{flash}</span>
        </div>
      )}

      {(state === "recording" || state === "processing") && (
        <div className="hud-stack">
          {partial && (
            <div className="hud-partial">
              <span>{partial}</span>
            </div>
          )}
          <div className={`hud-pill ${state === "processing" ? "is-processing" : ""}`}>
            <div className="hud-wave">
              {bars.map((v, i) => (
                <span key={i} className="hud-bar" style={{ height: `${6 + v * 20}px` }} />
              ))}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
