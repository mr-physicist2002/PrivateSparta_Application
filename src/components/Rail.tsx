import { House, Server, Rss, Settings } from "lucide-react";
import { useAppStore, type Screen } from "../state/store";

const ITEMS: Array<{ screen: Screen; label: string; Icon: typeof House }> = [
  { screen: "home", label: "Home", Icon: House },
  { screen: "servers", label: "Servers", Icon: Server },
  { screen: "subscriptions", label: "Subscriptions", Icon: Rss },
  { screen: "settings", label: "Settings", Icon: Settings },
];

export function Rail() {
  const screen = useAppStore((s) => s.screen);
  const setScreen = useAppStore((s) => s.setScreen);
  return (
    <nav className="flex w-14 shrink-0 flex-col items-center gap-1 border-r border-border py-3">
      {ITEMS.map(({ screen: target, label, Icon }) => (
        <button
          key={target}
          aria-label={label}
          title={label}
          onClick={() => setScreen(target)}
          className={`flex size-10 items-center justify-center rounded-card transition-colors duration-[140ms] ${
            screen === target
              ? "bg-accent-wash text-accent"
              : "text-text-muted hover:bg-bg-elevated hover:text-text-secondary"
          }`}
        >
          <Icon size={18} />
        </button>
      ))}
    </nav>
  );
}
