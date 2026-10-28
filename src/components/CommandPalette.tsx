import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Activity, Boxes, Cog, Download, FolderPlus, HeartPulse, Inbox, Library, Plus, RefreshCw, ScanSearch, Search } from "lucide-react";
import "../views/library.css";
import { useStore, type View } from "../lib/store";
import { Kbd } from "./ui";
import { AgentDots } from "../views/library/AgentDots";
import { emit, KIND_LABEL } from "../views/library/util";

interface Item {
  id: string;
  section: "Skills" | "Go to" | "Actions";
  label: string;
  sub?: string;
  icon: ReactNode;
  hint?: ReactNode;
  run: () => void;
}

/** Substring matches beat scattered subsequences; earlier matches beat later ones. */
function fuzzy(q: string, text: string): number {
  if (!q) return 1;
  const t = text.toLowerCase();
  const idx = t.indexOf(q);
  if (idx >= 0) return 200 - idx + (idx === 0 ? 100 : 0);
  let from = 0;
  let last = -2;
  let score = 0;
  for (const ch of q) {
    const f = t.indexOf(ch, from);
    if (f < 0) return 0;
    score += f === last + 1 ? 6 : 1;
    last = f;
    from = f + 1;
  }
  return score;
}

const VIEWS: { view: View; label: string; icon: typeof Library }[] = [
  { view: "library", label: "Library", icon: Library },
  { view: "insights", label: "Insights", icon: Activity },
  { view: "health", label: "Health", icon: HeartPulse },
  { view: "agents", label: "Agents", icon: Boxes },
  { view: "sync", label: "Sync", icon: RefreshCw },
  { view: "settings", label: "Settings", icon: Cog },
];

export function CommandPalette() {
  const s = useStore();
  const [q, setQ] = useState("");
  const [active, setActive] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);
  const close = () => s.setPalette(false);

  const items = useMemo<Item[]>(() => {
    const query = q.trim().toLowerCase();
    const inLibrary = (fn: () => void) => () => {
      s.go("library");
      setTimeout(fn, 60);
    };

    const skills = s.skills
      .map((sk) => ({ sk, score: Math.max(fuzzy(query, sk.name) * 2, query && sk.description.toLowerCase().includes(query) ? 40 : 0) }))
      .filter((x) => x.score > 0)
      .sort((a, b) => (query ? b.score - a.score : Number(b.sk.starred) - Number(a.sk.starred) || b.sk.usage.recent - a.sk.usage.recent))
      .slice(0, query ? 8 : 6)
      .map<Item>(({ sk }) => ({
        id: `skill:${sk.name}`,
        section: "Skills",
        label: sk.name,
        sub: sk.kind === "managed" ? sk.description : `${KIND_LABEL[sk.kind] ?? sk.kind} · ${sk.description}`,
        icon: <span className="palette-skill-dot" />,
        hint: <AgentDots agents={sk.agents} />,
        run: () => s.openSkill(sk.name),
      }));

    const views = VIEWS.map<Item>((v, i) => ({
      id: `view:${v.view}`,
      section: "Go to",
      label: v.label,
      icon: <v.icon size={14} />,
      hint: <Kbd>⌘{i + 1}</Kbd>,
      run: () => s.go(v.view),
    }));

    const actions: Item[] = [
      { id: "new", section: "Actions", label: "New skill", icon: <Plus size={14} />, hint: <Kbd>⌘N</Kbd>, run: () => s.setNewSkill(true) },
      { id: "import", section: "Actions", label: "Install skills from a repo", icon: <Download size={14} />, run: inLibrary(() => emit("upskill:import-repo")) },
      { id: "group", section: "Actions", label: "New group", icon: <FolderPlus size={14} />, run: inLibrary(() => emit("upskill:new-group")) },
      ...((s.status?.unmanaged ?? 0) > 0
        ? [{ id: "adopt", section: "Actions" as const, label: `Adopt ${s.status!.unmanaged} unmanaged skills`, icon: <Inbox size={14} />, run: () => void s.run("adopt_all", {}, "Adopted every unmanaged skill") }]
        : []),
      ...(s.status?.sync.kind ? [{ id: "sync", section: "Actions" as const, label: "Sync now", icon: <RefreshCw size={14} />, run: () => void s.run("sync_now", {}, "Synced") }] : []),
      {
        id: "rescan",
        section: "Actions",
        label: "Rescan agents and usage",
        icon: <ScanSearch size={14} />,
        run: async () => {
          await s.run("refresh");
          await s.run("ingest_usage", {}, "Rescanned agent folders and logs");
        },
      },
    ];

    const rank = (list: Item[]) =>
      query
        ? list
            .map((i) => ({ i, score: fuzzy(query, i.label) }))
            .filter((x) => x.score > 0)
            .sort((a, b) => b.score - a.score)
            .map((x) => x.i)
        : list;

    return [...skills, ...rank(actions), ...rank(views)];
  }, [q, s]);

  useEffect(() => setActive(0), [q]);
  useEffect(() => {
    listRef.current?.querySelector(".palette-item.on")?.scrollIntoView({ block: "nearest" });
  }, [active]);

  const choose = (i: Item | undefined) => {
    if (!i) return;
    close();
    i.run();
  };

  let lastSection = "";
  return (
    <div className="overlay" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div className="palette">
        <div className="palette-input">
          <Search size={17} className="faint" />
          <input
            autoFocus
            placeholder="Search skills, pages and actions…"
            value={q}
            spellCheck={false}
            onChange={(e) => setQ(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") {
                e.preventDefault();
                setActive((a) => Math.min(items.length - 1, a + 1));
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                setActive((a) => Math.max(0, a - 1));
              } else if (e.key === "Enter") {
                e.preventDefault();
                choose(items[active]);
              } else if (e.key === "Escape") {
                e.preventDefault();
                close();
              }
            }}
          />
          <Kbd>esc</Kbd>
        </div>
        <div className="palette-list" ref={listRef}>
          {items.length === 0 && <div className="palette-none">Nothing matches “{q}”.</div>}
          {items.map((it, i) => {
            const head = it.section !== lastSection ? it.section : null;
            lastSection = it.section;
            return (
              <div key={it.id}>
                {head && <div className="palette-section">{head}</div>}
                <div className={`palette-item${i === active ? " on" : ""}`} onMouseMove={() => i !== active && setActive(i)} onClick={() => choose(it)}>
                  <span className="palette-icon">{it.icon}</span>
                  <span className="palette-label">{it.label}</span>
                  {it.sub && <span className="palette-sub ellipsis">{it.sub}</span>}
                  <span className="grow" />
                  {it.hint}
                </div>
              </div>
            );
          })}
        </div>
        <div className="palette-foot">
          <span>
            <Kbd>↑</Kbd> <Kbd>↓</Kbd> to move
          </span>
          <span>
            <Kbd>↵</Kbd> to open
          </span>
          <span className="grow" />
          <span>UpSkill</span>
        </div>
      </div>
    </div>
  );
}
