import { useEffect, useState } from "react";
import { Config, getConfig, setConfig } from "./ipc";

/** Load config once; every update is optimistically applied and persisted. */
export function useConfig() {
  const [config, setLocal] = useState<Config | null>(null);

  useEffect(() => {
    getConfig().then(setLocal).catch(console.error);
  }, []);

  const update = (mutate: (c: Config) => void) => {
    setLocal((prev) => {
      if (!prev) return prev;
      const next = structuredClone(prev);
      mutate(next);
      setConfig(next).catch(console.error);
      return next;
    });
  };

  return { config, update };
}
