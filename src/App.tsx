import { useEffect } from "react";
import { TitleBar } from "./components/TitleBar";
import { Rail } from "./components/Rail";
import { Home } from "./screens/Home";
import { Servers } from "./screens/Servers";
import { Subscriptions } from "./screens/Subscriptions";
import { SettingsScreen } from "./screens/SettingsScreen";
import { ImportPreviewModal } from "./components/ImportPreviewModal";
import { Toasts } from "./components/Toasts";
import { useAppStore } from "./state/store";

export default function App() {
  const init = useAppStore((s) => s.init);
  const screen = useAppStore((s) => s.screen);
  useEffect(() => {
    void init();
  }, [init]);

  return (
    <div className="relative flex h-full flex-col">
      <TitleBar />
      <div className="flex min-h-0 flex-1">
        <Rail />
        {screen === "home" ? <Home /> : null}
        {screen === "servers" ? <Servers /> : null}
        {screen === "subscriptions" ? <Subscriptions /> : null}
        {screen === "settings" ? <SettingsScreen /> : null}
      </div>
      <ImportPreviewModal />
      <Toasts />
    </div>
  );
}
