import { useEffect, useState } from "react";
import { Config, getLevel, listDevices } from "../../lib/ipc";
import { PageHeader, Row, Section, Select, inputCls } from "../ui";

export default function Microphone({
  config,
  update,
}: {
  config: Config;
  update: (mutate: (c: Config) => void) => void;
}) {
  const [devices, setDevices] = useState<string[]>([]);
  const [level, setLevel] = useState(0);

  useEffect(() => {
    listDevices().then(setDevices).catch(console.error);
    const timer = setInterval(() => getLevel().then(setLevel).catch(() => {}), 100);
    return () => clearInterval(timer);
  }, []);

  const pct = Math.min(100, Math.round(level * 400));

  return (
    <div>
      <PageHeader title="Microphone" subtitle="Pick your input and check the level before dictating." />

      <Section title="Input device">
        <Row label="Device" hint="Changing devices takes effect immediately">
          <Select
            className="w-72"
            value={config.audio.input_device ?? ""}
            onChange={(v) => update((c) => void (c.audio.input_device = v || null))}
            options={[
              { value: "", label: "System default" },
              ...devices.map((d) => ({ value: d, label: d })),
            ]}
          />
        </Row>
        <Row label="Input level" hint="Speak — the bar should move">
          <div className="h-3 w-56 overflow-hidden rounded-full bg-neutral-800">
            <div
              className="h-full rounded-full bg-emerald-500 transition-[width] duration-100"
              style={{ width: `${pct}%` }}
            />
          </div>
        </Row>
        <Row
          label="Pre-roll"
          hint="Audio kept from before the hotkey press so the first syllable isn't clipped (ms)"
        >
          <input
            type="number"
            className={inputCls + " w-24"}
            value={config.audio.preroll_ms}
            min={0}
            max={1000}
            step={50}
            onChange={(e) =>
              update((c) => void (c.audio.preroll_ms = Number(e.target.value) || 0))
            }
          />
        </Row>
      </Section>
    </div>
  );
}
