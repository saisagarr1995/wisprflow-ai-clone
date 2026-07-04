import Assistant from "./assistant/Assistant";
import Hud from "./hud/Hud";
import Settings from "./settings/Settings";

export default function App() {
  // Each window loads the same bundle, routed by URL hash.
  const hash = window.location.hash;
  if (hash.includes("hud")) return <Hud />;
  if (hash.includes("assistant")) return <Assistant />;
  return <Settings />;
}
