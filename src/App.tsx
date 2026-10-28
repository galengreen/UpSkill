import { useEffect, useState } from "react";
import { Activity, Boxes, ChevronsUp, Cog, HeartPulse, Library as LibraryIcon, Plus, RefreshCw, Search } from "lucide-react";
import { StoreProvider, useStore, type View } from "./lib/store";
import { ago, Kbd } from "./components/ui";
import { Library } from "./views/Library";
import { Insights } from "./views/Insights";
import { Health } from "./views/Health";
import { Agents } from "./views/Agents";
import { Sync } from "./views/Sync";
import { Settings } from "./views/Settings";
import { CommandPalette } from "./components/CommandPalette";
import { NewSkillModal } from "./components/NewSkillModal";
import { Onboarding } from "./components/Onboarding";
import { GroupModal } from "./components/GroupModal";

export function App() {
  return (
    <StoreProvider>
      <Shell />
    </StoreProvider>
  );
}

const NAV: { view: View; label: string; icon: typeof LibraryIcon }[] = [
  { view: "library", label: "Library", icon: LibraryIcon },
  { view: "insights", label: "Insights", icon: Activity },
  { view: "health", label: "Health", icon: HeartPulse },
  { view: "agents", label: "Agents", icon: Boxes },
  { view: "sync", label: "Sync", icon: RefreshCw },
  { view: "settings", label: "Settings", icon: Cog },
];

function Shell() {
  const s = useStore();

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      if (!mod) return;
      if (e.key === "k") {
        e.preventDefault();
        s.setPalette(!s.palette);
      } else if (e.key === "n") {
        e.preventDefault();
        s.setNewSkill(true);
      } else if (/^[1-6]$/.test(e.key)) {
        e.preventDefault();
        s.go(NAV[Number(e.key) - 1].view);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [s]);

  return (
    <div className="app">
      <Sidebar />
      <main className="main">
        {s.view === "library" && <Library />}
        {s.view === "insights" && <Insights />}
        {s.view === "health" && <Health />}
        {s.view === "agents" && <Agents />}
        {s.view === "sync" && <Sync />}
        {s.view === "settings" && <Settings />}
      </main>
      {s.palette && <CommandPalette />}
      {s.newSkill && <NewSkillModal />}
      {s.loaded && s.status && !s.status.onboarded && <Onboarding />}
    </div>
  );
}

function Sidebar() {
  const s = useStore();
  const [groupModal, setGroupModal] = useState(false);
  const managed = s.skills.filter((x) => x.kind === "managed" && !x.archived).length;
  const issues = s.skills.reduce((n, x) => n + x.errors, 0);
  const sync = s.status?.sync;

  const counts: Partial<Record<View, React.ReactNode>> = {
    library: managed || null,
    health: issues ? <span style={{ color: "var(--error)" }}>{issues}</span> : null,
    agents: s.status?.agents.length || null,
  };

  return (
    <aside className="sidebar">
      <div className="drag-strip drag" data-tauri-drag-region />
      <div className="brand drag" data-tauri-drag-region>
        <span className="brand-mark">
          <ChevronsUp size={15} strokeWidth={2.6} />
        </span>
        <span className="brand-name">UpSkill</span>
        <span className="brand-sub" title="This machine">
          {s.status?.machine.name}
        </span>
      </div>

      <div className="search-trigger" onClick={() => s.setPalette(true)}>
        <Search size={13} />
        <span>Search</span>
        <Kbd>⌘K</Kbd>
      </div>

      <div className="sidebar-scroll">
        <nav className="nav">
          {NAV.map(({ view, label, icon: Icon }) => (
            <button
              key={view}
              className={`nav-item${s.view === view ? " active" : ""}`}
              onClick={() => {
                s.go(view);
                if (view === "library") s.setFilter({ group: null, kind: "all" });
              }}
            >
              <Icon size={15} strokeWidth={1.8} />
              {label}
              {counts[view] != null && <span className="count">{counts[view]}</span>}
            </button>
          ))}
        </nav>

        <div className="nav-section">
          Groups
          <button className="btn ghost icon sm" style={{ width: 20, height: 20 }} title="New group" onClick={() => setGroupModal(true)}>
            <Plus size={13} />
          </button>
        </div>
        <nav className="nav">
          {s.groups.length === 0 && <div className="faint" style={{ padding: "4px 9px", fontSize: 12 }}>No groups yet</div>}
          {s.groups.map((g) => (
            <button
              key={g.id}
              className={`nav-item${s.view === "library" && s.filter.group === g.id ? " active" : ""}`}
              onClick={() => {
                s.go("library");
                s.setFilter({ group: g.id, kind: "all" });
              }}
            >
              <span className="dot" style={{ background: g.color }} />
              <span className="ellipsis">{g.name}</span>
              <span className="count">{g.count}</span>
            </button>
          ))}
        </nav>
      </div>

      <div className="sidebar-foot">
        <span className="dot" style={{ background: sync?.error ? "var(--error)" : sync?.kind ? "var(--accent)" : "var(--text-4)" }} />
        <span className="ellipsis grow" title={sync?.error ?? undefined}>
          {sync?.error ? "Sync failed" : sync?.kind ? `Synced ${ago(sync.last)}` : "Sync off"}
        </span>
        <button className="btn ghost icon sm" title="New skill (⌘N)" onClick={() => s.setNewSkill(true)}>
          <Plus size={14} />
        </button>
      </div>
      {groupModal && <GroupModal group={null} onClose={() => setGroupModal(false)} />}
    </aside>
  );
}
