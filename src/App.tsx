import { useEffect } from "react";
import { TitleBar } from "./components/TitleBar";
import { Home } from "./screens/Home";
import { ImportPreviewModal } from "./components/ImportPreviewModal";
import { Toasts } from "./components/Toasts";
import { useAppStore } from "./state/store";

export default function App() {
  const init = useAppStore((s) => s.init);
  useEffect(() => {
    void init();
  }, [init]);

  return (
    <div className="relative flex h-full flex-col">
      <TitleBar />
      <Home />
      <ImportPreviewModal />
      <Toasts />
    </div>
  );
}
