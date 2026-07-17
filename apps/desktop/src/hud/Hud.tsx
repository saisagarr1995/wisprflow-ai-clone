import { useEffect, useRef, useState, type CSSProperties } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { Config, getConfig, hudCancel, hudFinish, hudStart, hudZones } from "../lib/ipc";
import "./hud.css";

const log = (msg: string) => invoke("ui_log", { msg }).catch(() => {});

type HudState = "idle" | "recording" | "processing" | "flash";

const BAR_COUNT = 14;
const IDLE_BARS = Array(BAR_COUNT).fill(0.12);

/** "Ctrl+Win" → "Ctrl + Win" for the idle hint pill. */
const prettyChord = (chord: string) => chord.split("+").join(" + ");

/**
 * The Wispr Flow-style floating pill. Minimal and compact:
 *  - idle: a tiny handle bar; hovering it expands a hint pill
 *    ("Click or hold Ctrl + Win to start dictating"); clicking starts.
 *  - recording: a dark capsule with a cancel (X) button, a live waveform and
 *    a finish-and-paste (stop) button; a text pill with the streaming partial
 *    floats above it once words arrive.
 *  - processing: the waveform gently pulses while the final pass + LLM run.
 *  - flash: a brief confirmation pill after a voice command.
 * The window never takes focus; Rust-side hit-testing makes only the zones
 * this component reports clickable, everything else stays click-through.
 */
export default function Hud() {
  const [state, setState] = useState<HudState>("idle");
  const [hovered, setHovered] = useState(false);
  const [chord, setChord] = useState("RCtrl");
  const [partial, setPartial] = useState("");
  const [flash, setFlash] = useState("");
  const [bars, setBars] = useState<number[]>(IDLE_BARS);
  const levelsRef = useRef<number[]>([...IDLE_BARS]);
  const peakRef = useRef(0.02);
  const flashTimer = useRef<number | undefined>(undefined);
  const zoneRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const unsubs: Array<() => void> = [];
    log("hud mounted");

    getConfig()
      .then((c) => setChord(c.hotkey.chord))
      .catch(() => {});
    listen<Config>("config", (e) => setChord(e.payload.hotkey.chord)).then((u) =>
      unsubs.push(u),
    );

    listen<string>("status", (e) => {
      const s = e.payload as "idle" | "recording" | "processing";
      log(`hud status=${s}`);
      setState(s);
      setHovered(false);
      if (s === "recording") {
        setPartial("");
        setFlash("");
        levelsRef.current = [...IDLE_BARS];
        peakRef.current = 0.02;
        setBars([...IDLE_BARS]);
      }
    }).then((u) => unsubs.push(u));

    listen<number>("level", (e) => {
      // Auto-gain: normalise RMS against a decaying peak so speech visibly
      // waves at any mic gain, while true silence stays at the floor.
      peakRef.current = Math.max(e.payload, peakRef.current * 0.94, 0.02);
      const v = Math.min(1, Math.max(0.12, Math.pow(e.payload / peakRef.current, 0.7)));
      const next = [...levelsRef.current.slice(1), v];
      levelsRef.current = next;
      setBars(next);
    }).then((u) => unsubs.push(u));

    listen<string>("partial", (e) => {
      // Keep the newest words visible: the bubble is one line and its CSS
      // ellipsis would hide the end of long text, which is where speech is.
      const t = e.payload.trim();
      if (/^[[(].*[\])]$/.test(t)) return; // stray non-speech annotation
      setPartial(t.length > 58 ? "…" + t.slice(-58) : t);
    }).then((u) => unsubs.push(u));

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

  // Report the clickable region (idle pill / recording capsule) after every
  // layout change; Rust polls the cursor against these to decide when the
  // otherwise click-through window should accept the mouse. Processing and
  // flash have nothing to click.
  useEffect(() => {
    const el = zoneRef.current;
    if ((state !== "idle" && state !== "recording") || !el) {
      hudZones([]).catch(() => {});
      return;
    }
    const r = el.getBoundingClientRect();
    // A little slack so the zone survives the hover-expansion animation and
    // near-miss clicks.
    hudZones([
      { x: r.left - 6, y: r.top - 6, width: r.width + 12, height: r.height + 12 },
    ]).catch(() => {});
  }, [state, hovered, partial]);

  return (
    <div className="hud-root">
      {state === "idle" && (
        <div
          ref={zoneRef}
          className={`hud-idle ${hovered ? "is-hovered" : ""}`}
          onMouseEnter={() => setHovered(true)}
          onMouseLeave={() => setHovered(false)}
          onClick={() => {
            log("hud idle pill clicked");
            hudStart().catch(() => {});
          }}
          role="button"
          title=""
        >
          {hovered ? (
            <span className="hud-hint">
              Click or hold <b>{prettyChord(chord)}</b> to start dictating
            </span>
          ) : (
            <div className="hud-handle" />
          )}
        </div>
      )}

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
          <div
            ref={state === "recording" ? zoneRef : undefined}
            className={`hud-pill ${state === "processing" ? "is-processing" : ""}`}
          >
            {state === "recording" && (
              <button
                className="hud-btn hud-btn-cancel"
                data-tip="Cancel"
                onClick={() => {
                  log("hud cancel clicked");
                  hudCancel().catch(() => {});
                }}
                aria-label="Cancel"
              >
                <svg viewBox="0 0 20 20" aria-hidden>
                  <path
                    d="M6 6l8 8M14 6l-8 8"
                    stroke="currentColor"
                    strokeWidth="2"
                    strokeLinecap="round"
                  />
                </svg>
              </button>
            )}
            <div className="hud-wave">
              {bars.map((v, i) => (
                <span
                  key={i}
                  className="hud-bar"
                  style={{ height: `${6 + v * 20}px`, "--i": i } as CSSProperties}
                />
              ))}
            </div>
            {state === "recording" && (
              <button
                className="hud-btn hud-btn-finish"
                data-tip="Finish and paste"
                onClick={() => {
                  log("hud finish clicked");
                  hudFinish().catch(() => {});
                }}
                aria-label="Finish and paste"
              >
                <svg viewBox="0 0 20 20" aria-hidden>
                  <rect x="6" y="6" width="8" height="8" rx="1.5" fill="currentColor" />
                </svg>
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
