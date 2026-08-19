import { House, Server, Rss, ScrollText, Settings } from "lucide-react";
import { useAppStore, type Screen } from "../state/store";
import { useT, type MessageKey } from "../i18n";

const ITEMS: Array<{ screen: Screen; labelKey: MessageKey; Icon: typeof House }> = [
  { screen: "home", labelKey: "navHome", Icon: House },
  { screen: "servers", labelKey: "navServers", Icon: Server },
  { screen: "subscriptions", labelKey: "navSubscriptions", Icon: Rss },
  { screen: "logs", labelKey: "navLogs", Icon: ScrollText },
  { screen: "settings", labelKey: "navSettings", Icon: Settings },
];

export function Rail() {
  const t = useT();
  const screen = useAppStore((s) => s.screen);
  const setScreen = useAppStore((s) => s.setScreen);
  return (
    <nav className="flex w-14 shrink-0 flex-col items-center gap-1 border-e border-border py-3">
      {ITEMS.map(({ screen: target, labelKey, Icon }) => (
        <button
          type="button"
          key={target}
          aria-label={t(labelKey)}
          aria-current={screen === target ? "page" : undefined}
          title={t(labelKey)}
          onClick={() => setScreen(target)}
          className={`flex size-10 items-center justify-center rounded-card transition-colors duration-[140ms] ${
            screen === target
              ? "bg-accent-wash text-accent ring-1 ring-inset ring-accent/20"
              : "text-text-muted hover:bg-bg-elevated hover:text-text-secondary"
          }`}
        >
          <Icon size={18} />
        </button>
      ))}
    </nav>
  );
}
