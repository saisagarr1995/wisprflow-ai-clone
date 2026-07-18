import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  Config,
  downloadModel,
  listModels,
  ModelInfo,
  ModelProgress,
  OllamaStatus,
  ollamaStatus,
} from "../../lib/ipc";
import { PageHeader, Row, Section, Toggle, btnCls, inputCls } from "../ui";

function mb(bytes: number) {
  return `${Math.round(bytes / 1024 / 1024)} MB`;
}

export default function Models({
  config,
  update,
}: {
  config: Config;
  update: (mutate: (c: Config) => void) => void;
}) {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [progress, setProgress] = useState<Record<string, ModelProgress>>({});
  const [ollama, setOllama] = useState<OllamaStatus | null>(null);

  const refresh = () => {
    listModels().then(setModels).catch(console.error);
    ollamaStatus().then(setOllama).catch(console.error);
  };

  useEffect(() => {
    refresh();
    let unsub: (() => void) | undefined;
    listen<ModelProgress>("model-progress", (e) => {
      setProgress((p) => ({ ...p, [e.payload.id]: e.payload }));
      if (e.payload.done) refresh();
    }).then((u) => (unsub = u));
    return () => unsub?.();
  }, []);

  const sttModels = models.filter((m) => m.kind === "stt");

  return (
    <div>
      <PageHeader
        title="Models & AI"
        subtitle="Your on-device speech model and the local Ollama models that clean up and transform text."
      />

      <Section title="Speech-to-text">
        {sttModels.map((m) => {
          const p = progress[m.id];
          const downloading = p && !p.done;
          const active = config.stt.model_id === m.id;
          return (
            <Row
              key={m.id}
              label={m.id}
              hint={`${mb(m.size_bytes)} · ${m.languages}${p?.error ? ` · ${p.error}` : ""}`}
            >
              <div className="flex items-center gap-3">
                {downloading && (
                  <div className="h-2 w-28 overflow-hidden rounded-full bg-neutral-800">
                    <div
                      className="h-full bg-indigo-500"
                      style={{ width: `${Math.round((p.downloaded / Math.max(1, p.total)) * 100)}%` }}
                    />
                  </div>
                )}
                {!m.installed && !downloading && (
                  <button className={btnCls} onClick={() => downloadModel(m.id)}>
                    Download
                  </button>
                )}
                <button
                  disabled={!m.installed && !active}
                  onClick={() => update((c) => void (c.stt.model_id = m.id))}
                  className={`rounded-lg px-3 py-1.5 text-sm font-medium ${
                    active
                      ? "bg-emerald-700 text-white"
                      : "border border-neutral-700 text-neutral-300 hover:bg-neutral-800 disabled:opacity-40"
                  }`}
                >
                  {active ? "Active" : "Use"}
                </button>
              </div>
            </Row>
          );
        })}
        <Row
          label="GPU transcription"
          hint="Decode on the graphics card (Vulkan). Faster on discrete GPUs; integrated laptop GPUs are usually SLOWER than the CPU — leave off unless you measure a win. Takes effect on restart."
        >
          <Toggle
            value={config.stt.gpu}
            onChange={(v) => update((c) => void (c.stt.gpu = v))}
          />
        </Row>
      </Section>

      <Section title="AI cleanup (Ollama)">
        <Row
          label="Enable AI cleanup"
          hint="Removes filler words, fixes punctuation, formats lists — fully local via Ollama"
        >
          <Toggle
            value={config.llm.enabled}
            onChange={(v) => update((c) => void (c.llm.enabled = v))}
          />
        </Row>
        <Row label="Ollama URL">
          <input
            className={inputCls}
            value={config.llm.base_url}
            onChange={(e) => update((c) => void (c.llm.base_url = e.target.value))}
            spellCheck={false}
          />
        </Row>
        <Row label="Cleanup model" hint="Small + fast; runs on every dictation">
          <input
            className={inputCls}
            value={config.llm.cleanup_model}
            onChange={(e) => update((c) => void (c.llm.cleanup_model = e.target.value))}
            spellCheck={false}
          />
        </Row>
        <Row label="Rewrite model" hint="Larger; used for voice commands and rewrites">
          <input
            className={inputCls}
            value={config.llm.rewrite_model}
            onChange={(e) => update((c) => void (c.llm.rewrite_model = e.target.value))}
            spellCheck={false}
          />
        </Row>
        <Row
          label="Status"
          hint={
            ollama?.reachable
              ? ollama.missing.length
                ? `Missing models — run: ollama pull ${ollama.missing.join(" && ollama pull ")}`
                : "All required models are available"
              : (ollama?.error ?? "Checking…")
          }
        >
          <div className="flex items-center gap-2">
            <span
              className={`inline-block h-2.5 w-2.5 rounded-full ${
                ollama?.reachable
                  ? ollama.missing.length
                    ? "bg-amber-500"
                    : "bg-emerald-500"
                  : "bg-red-500"
              }`}
            />
            <button
              className="rounded-lg border border-neutral-700 px-3 py-1.5 text-sm text-neutral-300 hover:bg-neutral-800"
              onClick={refresh}
            >
              Refresh
            </button>
          </div>
        </Row>
      </Section>
    </div>
  );
}
