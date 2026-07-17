import { invoke } from "@tauri-apps/api/core";

export interface Config {
  version: number;
  language: string;
  hotkey: { chord: string; mode: "hold" | "toggle" | "hands_free" };
  audio: { input_device: string | null; preroll_ms: number };
  stt: { engine: "auto" | "parakeet" | "whisper"; model_id: string };
  llm: {
    enabled: boolean;
    base_url: string;
    cleanup_model: string;
    rewrite_model: string;
    coding_model: string | null;
  };
  injection: { strategy: string; clipboard_restore_delay_ms: number };
}

export interface ModelInfo {
  id: string;
  kind: string;
  size_bytes: number;
  languages: string;
  installed: boolean;
}

export interface ModelProgress {
  id: string;
  downloaded: number;
  total: number;
  done: boolean;
  error: string | null;
}

export interface OllamaStatus {
  reachable: boolean;
  available: string[];
  missing: string[];
  error: string | null;
}

export const getConfig = () => invoke<Config>("get_config");
export const setConfig = (config: Config) => invoke<void>("set_config", { config });
export const listDevices = () => invoke<string[]>("list_devices");
export const getLevel = () => invoke<number>("get_level");
export const listModels = () => invoke<ModelInfo[]>("list_models");
export const downloadModel = (id: string) => invoke<void>("download_model", { id });
export const ollamaStatus = () => invoke<OllamaStatus>("ollama_status");

// HUD pill interactions (start / finish-and-paste / cancel) and the
// clickable-zone report that drives selective click-through.
export interface HudZone {
  x: number;
  y: number;
  width: number;
  height: number;
}
export const hudStart = () => invoke<void>("hud_start");
export const hudFinish = () => invoke<void>("hud_finish");
export const hudCancel = () => invoke<void>("hud_cancel");
export const hudZones = (zones: HudZone[]) => invoke<void>("hud_zones", { zones });
