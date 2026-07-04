import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Section, btnCls, inputCls, selectCls } from "../ui";

interface CustomCommand {
  id: number;
  name: string;
  triggers: string;
  prompt: string;
  output_mode: string;
}

const BUILTINS = [
  "rewrite professionally",
  "summarize",
  "make it shorter",
  "expand this",
  "fix grammar",
  "turn this into an email",
  "make a bullet list",
  "convert to markdown",
  "translate to …",
  "explain this",
  "ask local AI …",
];

export default function Commands() {
  const [cmds, setCmds] = useState<CustomCommand[]>([]);
  const [name, setName] = useState("");
  const [triggers, setTriggers] = useState("");
  const [prompt, setPrompt] = useState("Rewrite the following: {input}");
  const [outputMode, setOutputMode] = useState("replace");

  const refresh = () => invoke<CustomCommand[]>("commands_list").then(setCmds).catch(console.error);
  useEffect(() => {
    refresh();
  }, []);

  const add = async () => {
    if (!name.trim() || !triggers.trim() || !prompt.includes("{input}")) return;
    await invoke("commands_add", { name: name.trim(), triggers, prompt, outputMode });
    setName("");
    setTriggers("");
    refresh();
  };

  return (
    <div className="max-w-2xl">
      <h1 className="mb-2 text-xl font-semibold">Voice commands</h1>
      <p className="mb-6 text-sm text-neutral-500">
        Select text (or dictate first), then hold your hotkey and say a command. It runs through
        your local model and replaces the selection.
      </p>

      <Section title="Built-in commands">
        <div className="flex flex-wrap gap-2 px-4 py-3">
          {BUILTINS.map((b) => (
            <span key={b} className="rounded-full bg-neutral-800 px-3 py-1 text-xs text-neutral-300">
              {b}
            </span>
          ))}
        </div>
      </Section>

      <h2 className="mb-3 mt-8 text-sm font-semibold uppercase tracking-wider text-neutral-500">
        Your commands
      </h2>
      <div className="mb-4 flex flex-col gap-2 rounded-xl border border-neutral-800 bg-neutral-900/60 p-4">
        <div className="flex gap-2">
          <input
            className={inputCls + " w-40"}
            placeholder="Name"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <input
            className={inputCls + " flex-1"}
            placeholder="Triggers, comma-separated (make it pirate, piratify)"
            value={triggers}
            onChange={(e) => setTriggers(e.target.value)}
          />
          <select className={selectCls} value={outputMode} onChange={(e) => setOutputMode(e.target.value)}>
            <option value="replace">Replace selection</option>
            <option value="popover">Show in popover</option>
          </select>
        </div>
        <textarea
          className={inputCls + " h-20 w-full resize-y font-mono text-xs"}
          placeholder="Prompt — must contain {input}"
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          spellCheck={false}
        />
        {!prompt.includes("{input}") && (
          <div className="text-xs text-amber-500">Prompt must contain {"{input}"}.</div>
        )}
        <button className={btnCls + " w-fit"} onClick={add}>
          Add command
        </button>
      </div>

      <div className="flex flex-col gap-2">
        {cmds.map((c) => (
          <div
            key={c.id}
            className="flex items-start justify-between gap-4 rounded-xl border border-neutral-800 bg-neutral-900/60 px-4 py-3"
          >
            <div className="min-w-0">
              <div className="text-sm font-medium text-neutral-200">
                {c.name}
                <span className="ml-2 text-xs font-normal text-neutral-500">{c.output_mode}</span>
              </div>
              <div className="mt-0.5 truncate text-xs text-neutral-500">“{c.triggers}”</div>
            </div>
            <button
              className="shrink-0 text-xs text-neutral-500 hover:text-red-400"
              onClick={() => invoke("commands_delete", { id: c.id }).then(refresh)}
            >
              Remove
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
