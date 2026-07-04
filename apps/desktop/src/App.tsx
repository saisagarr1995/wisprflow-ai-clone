import Hud from "./hud/Hud";
import Settings from "./settings/Settings";

export default function App() {
  // The HUD window loads the same bundle with "#hud" in its URL.
  const isHud = window.location.hash.includes("hud");
  return isHud ? <Hud /> : <Settings />;
}
