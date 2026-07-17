import { Config } from "../../lib/ipc";
import { PageHeader, Row, Section, Select, inputCls } from "../ui";

const HOTKEY_PRESETS = ["Ctrl+Win", "RCtrl", "RAlt", "F9", "Ctrl+Shift+Space", "Ctrl+Shift+D"];
const LANGUAGES: Array<[string, string]> = [
  ["auto", "Auto-detect"],
  ["en", "English"],
  ["ta", "Tamil"],
  ["te", "Telugu"],
  ["hi", "Hindi"],
  ["de", "German"],
  ["fr", "French"],
  ["es", "Spanish"],
  ["ja", "Japanese"],
  ["zh", "Chinese"],
];

export default function General({
  config,
  update,
}: {
  config: Config;
  update: (mutate: (c: Config) => void) => void;
}) {
  return (
    <div>
      <PageHeader
        title="General"
        subtitle="Your dictation hotkey, language, and how text is inserted."
      />

      <Section title="Dictation hotkey">
        <Row label="Hotkey" hint="Preset chords; or type a custom one like Alt+Z">
          <div className="flex gap-2">
            <Select
              className="w-44"
              value={HOTKEY_PRESETS.includes(config.hotkey.chord) ? config.hotkey.chord : "custom"}
              onChange={(v) => {
                if (v !== "custom") {
                  update((c) => void (c.hotkey.chord = v));
                }
              }}
              options={[
                ...HOTKEY_PRESETS.map((p) => ({ value: p, label: p })),
                { value: "custom", label: "Custom…" },
              ]}
            />
            <input
              className={inputCls + " w-40"}
              value={config.hotkey.chord}
              onChange={(e) => update((c) => void (c.hotkey.chord = e.target.value))}
              spellCheck={false}
            />
          </div>
        </Row>
        <Row
          label="Mode"
          hint="Hold: press and hold to talk. Toggle: press once to start, again to stop. Hands-free: press once (or click the pill), it stops by itself when you go quiet."
        >
          <Select
            className="w-52"
            value={config.hotkey.mode}
            onChange={(v) =>
              update((c) => void (c.hotkey.mode = v as "hold" | "toggle" | "hands_free"))
            }
            options={[
              { value: "hold", label: "Hold (push-to-talk)" },
              { value: "toggle", label: "Toggle" },
              { value: "hands_free", label: "Hands-free (auto-stop)" },
            ]}
          />
        </Row>
      </Section>

      <Section title="Language">
        <Row label="Spoken language" hint="Auto-detect works well; forcing a language is faster">
          <Select
            className="w-44"
            value={config.language}
            onChange={(v) => update((c) => void (c.language = v))}
            options={LANGUAGES.map(([id, label]) => ({ value: id, label }))}
          />
        </Row>
        {(config.language !== "en" || config.stt.model_id.endsWith("-multi")) && (
          <p className="mt-2 text-xs text-zinc-400">
            Non-English and auto-detect need a <b>multilingual</b> speech model — pick{" "}
            <code>whisper-small-multi</code> (best for Tamil/Telugu) or{" "}
            <code>whisper-base-multi</code> (faster) under <b>Models &amp; AI</b>. The default{" "}
            <code>whisper-base</code> model is English-only.
          </p>
        )}
      </Section>

      <Section title="Injection">
        <Row
          label="Clipboard restore delay"
          hint="How long slow apps (Teams, Outlook) get to read the paste, in ms"
        >
          <input
            type="number"
            className={inputCls + " w-24"}
            value={config.injection.clipboard_restore_delay_ms}
            min={0}
            max={2000}
            onChange={(e) =>
              update(
                (c) => void (c.injection.clipboard_restore_delay_ms = Number(e.target.value) || 0),
              )
            }
          />
        </Row>
      </Section>
    </div>
  );
}
