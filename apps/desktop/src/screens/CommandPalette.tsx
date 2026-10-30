import { useEffect, useMemo, useRef, useState } from "react";
import { Icon } from "../components/Icon";
import { Avatar } from "../components/ui";
import type { EntrySummary } from "../lib/api";
import { hostOf } from "../lib/utils";
import { useT } from "../i18n";
import type { Tab } from "./Shell";

type Item =
  | { kind: "entry"; entry: EntrySummary }
  | { kind: "action"; icon: string; label: string; run: () => void };

/**
 * ⌘K / Ctrl+K: find an entry and copy its password without touching the mouse
 * (Enter), or open it (⌘Enter); a few actions are listed too. Desktop only.
 */
export default function CommandPalette(props: {
  entries: EntrySummary[];
  onClose: () => void;
  onOpen: (id: string) => void;
  onCopyPassword: (id: string) => void;
  onNew: () => void;
  onGo: (tab: Tab) => void;
  onLock: () => void;
}) {
  const { t } = useT();
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const list = useRef<HTMLDivElement>(null);

  const items = useMemo<Item[]>(() => {
    const q = query.trim().toLowerCase();
    const match = (s: string) => !q || s.toLowerCase().includes(q);
    const entries = props.entries
      .filter((e) => match(e.title) || match(e.username) || e.urls.some(match))
      .slice(0, 8)
      .map((entry) => ({ kind: "entry" as const, entry }));
    const actions: Item[] = (
      [
        // icons: add casino health_and_safety lock
        { kind: "action", icon: "add", label: t("home.actionAdd"), run: props.onNew },
        { kind: "action", icon: "casino", label: t("shell.navGenerator"), run: () => props.onGo("generator") },
        { kind: "action", icon: "health_and_safety", label: t("shell.navSecurity"), run: () => props.onGo("security") },
        { kind: "action", icon: "lock", label: t("shell.navLock"), run: props.onLock },
      ] as Item[]
    ).filter((a) => a.kind === "action" && match(a.label));
    return [...entries, ...actions];
  }, [query, props, t]);

  useEffect(() => setIndex(0), [query]);
  useEffect(() => {
    list.current?.querySelector(".pal-row.on")?.scrollIntoView({ block: "nearest" });
  }, [index]);

  const run = (item: Item | undefined, open: boolean) => {
    if (!item) return;
    props.onClose();
    if (item.kind === "action") item.run();
    else if (open) props.onOpen(item.entry.id);
    else props.onCopyPassword(item.entry.id);
  };

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") (e.preventDefault(), setIndex((i) => Math.min(i + 1, items.length - 1)));
    else if (e.key === "ArrowUp") (e.preventDefault(), setIndex((i) => Math.max(i - 1, 0)));
    else if (e.key === "Enter") (e.preventDefault(), run(items[index], e.metaKey || e.ctrlKey));
    else if (e.key === "Escape") props.onClose();
  };

  const firstAction = items.findIndex((i) => i.kind === "action");
  return (
    <div className="scrim pal-scrim" onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}>
      <div className="pal" role="dialog" aria-modal="true" onKeyDown={onKey}>
        <div className="pal-input">
          <Icon name="search" size={20} />
          <input autoFocus placeholder={t("palette.placeholder")} value={query} onChange={(e) => setQuery(e.target.value)} />
          <kbd className="kbd">esc</kbd>
        </div>
        <div className="pal-list" ref={list}>
          {items.length === 0 && <p className="muted pal-none">{t("home.noResults")}</p>}
          {items.map((item, i) => (
            <div key={item.kind === "entry" ? item.entry.id : item.label}>
              {i === 0 && item.kind === "entry" && <div className="pal-group">{t("palette.entries")}</div>}
              {i === firstAction && <div className="pal-group">{t("palette.actions")}</div>}
              <button className={`pal-row ${i === index ? "on" : ""}`} onMouseMove={() => setIndex(i)} onClick={() => run(item, item.kind === "entry")}>
                {item.kind === "entry" ? (
                  <>
                    <Avatar name={item.entry.title} urls={item.entry.urls} />
                    <span className="row-title">{item.entry.title}</span>
                    <span className="muted pal-sub">{item.entry.username || hostOf(item.entry.urls[0])}</span>
                  </>
                ) : (
                  <>
                    <span className="pal-icon"><Icon name={item.icon} size={18} /></span>
                    <span className="row-title">{item.label}</span>
                  </>
                )}
              </button>
            </div>
          ))}
        </div>
        <div className="pal-foot">
          <span><kbd className="kbd">↑↓</kbd> {t("palette.choose")}</span>
          <span><kbd className="kbd">↵</kbd> {t("palette.copy")}</span>
          <span><kbd className="kbd">⌘↵</kbd> {t("palette.open")}</span>
        </div>
      </div>
    </div>
  );
}
