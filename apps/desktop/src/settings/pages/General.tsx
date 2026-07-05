import { Config } from "../../lib/ipc";
import { PageHeader, Row, Section, selectCls, inputCls } from "../ui";

const HOTKEY_PRESETS = ["RCtrl", "RAlt", "F9", "Ctrl+Shift+Space", "Ctrl+Shift+D"];
const LANGUAGES: Array<[string, string]> = [
  ["auto", "Auto-detect"],
  ["en", "English"],
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
            <select
              className={selectCls}
              value={HOTKEY_PRESETS.includes(config.hotkey.chord) ? config.hotkey.chord : "custom"}
              onChange={(e) => {
                if (e.target.value !== "custom") {
                  update((c) => void (c.hotkey.chord = e.target.value));
                }
              }}
            >
              {HOTKEY_PRESETS.map((p) => (
                <option key={p} value={p}>
                  {p}
                </option>
              ))}
              <option value="custom">Custom…</option>
            </select>
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
          hint="Hold: press and hold to talk. Toggle: press once to start, again to stop."
        >
          <select
            className={selectCls}
            value={config.hotkey.mode}
            onChange={(e) =>
              update((c) => void (c.hotkey.mode = e.target.value as "hold" | "toggle"))
            }
          >
            <option value="hold">Hold (push-to-talk)</option>
            <option value="toggle">Toggle</option>
          </select>
        </Row>
      </Section>

      <Section title="Language">
        <Row label="Spoken language" hint="Auto-detect works well; forcing a language is faster">
          <select
            className={selectCls}
            value={config.language}
            onChange={(e) => update((c) => void (c.language = e.target.value))}
          >
            {LANGUAGES.map(([id, label]) => (
              <option key={id} value={id}>
                {label}
              </option>
            ))}
          </select>
        </Row>
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
