import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Section, btnCls, inputCls } from "../ui";

interface Snippet {
  id: number;
  trigger: string;
  expansion: string;
}

export default function Snippets() {
  const [snippets, setSnippets] = useState<Snippet[]>([]);
  const [trigger, setTrigger] = useState("");
  const [expansion, setExpansion] = useState("");

  const refresh = () => invoke<Snippet[]>("snippets_list").then(setSnippets).catch(console.error);
  useEffect(() => {
    refresh();
  }, []);

  const add = async () => {
    if (!trigger.trim() || !expansion.trim()) return;
    await invoke("snippets_add", { trigger: trigger.trim(), expansion });
    setTrigger("");
    setExpansion("");
    refresh();
  };

  return (
    <div className="max-w-2xl">
      <h1 className="mb-2 text-xl font-semibold">Snippets</h1>
      <p className="mb-6 text-sm text-neutral-500">
        Say the trigger phrase (optionally prefixed with &ldquo;insert&rdquo;), get the full text.
        Say <span className="text-neutral-300">“my address”</span> → your address appears.
      </p>

      <div className="mb-6 flex flex-col gap-2">
        <input
          className={inputCls + " w-72"}
          placeholder="Trigger phrase (my address)"
          value={trigger}
          onChange={(e) => setTrigger(e.target.value)}
          spellCheck={false}
        />
        <textarea
          className={inputCls + " h-24 w-full resize-y font-mono text-xs"}
          placeholder="Expansion text…"
          value={expansion}
          onChange={(e) => setExpansion(e.target.value)}
          spellCheck={false}
        />
        <button className={btnCls + " w-fit"} onClick={add}>
          Add snippet
        </button>
      </div>

      <Section title={`${snippets.length} snippets`}>
        {snippets.length === 0 && (
          <div className="px-4 py-6 text-center text-sm text-neutral-500">
            No snippets yet. Great for addresses, sign-offs, and boilerplate.
          </div>
        )}
        {snippets.map((s) => (
          <div key={s.id} className="flex items-start justify-between gap-4 px-4 py-2.5">
            <div className="min-w-0">
              <div className="text-sm font-medium text-neutral-200">“{s.trigger}”</div>
              <div className="mt-0.5 truncate text-xs text-neutral-500">{s.expansion}</div>
            </div>
            <button
              className="shrink-0 text-xs text-neutral-500 hover:text-red-400"
              onClick={() => invoke("snippets_delete", { id: s.id }).then(refresh)}
            >
              Remove
            </button>
          </div>
        ))}
      </Section>
    </div>
  );
}
