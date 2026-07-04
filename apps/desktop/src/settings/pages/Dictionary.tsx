import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Section, btnCls, inputCls } from "../ui";

interface DictEntry {
  id: number;
  word: string;
  hint: string | null;
}

export default function Dictionary() {
  const [entries, setEntries] = useState<DictEntry[]>([]);
  const [word, setWord] = useState("");
  const [hint, setHint] = useState("");

  const refresh = () =>
    invoke<DictEntry[]>("dictionary_list").then(setEntries).catch(console.error);
  useEffect(() => {
    refresh();
  }, []);

  const add = async () => {
    if (!word.trim()) return;
    await invoke("dictionary_add", { word: word.trim(), hint: hint.trim() || null });
    setWord("");
    setHint("");
    refresh();
  };

  return (
    <div className="max-w-2xl">
      <h1 className="mb-2 text-xl font-semibold">Dictionary</h1>
      <p className="mb-6 text-sm text-neutral-500">
        Words the recognizer should always get right — names, jargon, commands. The optional
        &ldquo;sounds like&rdquo; teaches it what the word sounds like when misheard (e.g.
        kubectl → &ldquo;cube control&rdquo;).
      </p>

      <div className="mb-6 flex gap-2">
        <input
          className={inputCls + " w-48"}
          placeholder="Word (kubectl)"
          value={word}
          onChange={(e) => setWord(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
          spellCheck={false}
        />
        <input
          className={inputCls + " w-56"}
          placeholder="Sounds like (cube control)"
          value={hint}
          onChange={(e) => setHint(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && add()}
          spellCheck={false}
        />
        <button className={btnCls} onClick={add}>
          Add
        </button>
      </div>

      <Section title={`${entries.length} words`}>
        {entries.length === 0 && (
          <div className="px-4 py-6 text-center text-sm text-neutral-500">
            No words yet. Add your name, your company, your tools.
          </div>
        )}
        {entries.map((e) => (
          <div key={e.id} className="flex items-center justify-between px-4 py-2.5">
            <div>
              <span className="text-sm font-medium text-neutral-200">{e.word}</span>
              {e.hint && <span className="ml-3 text-xs text-neutral-500">≈ “{e.hint}”</span>}
            </div>
            <button
              className="text-xs text-neutral-500 hover:text-red-400"
              onClick={() => invoke("dictionary_delete", { id: e.id }).then(refresh)}
            >
              Remove
            </button>
          </div>
        ))}
      </Section>
    </div>
  );
}
