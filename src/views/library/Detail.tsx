import { useCallback, useEffect, useState } from "react";
import { Activity, Archive, ArchiveRestore, Clock, Coins, Ellipsis, FolderOpen, GitCommitVertical, Pencil, Star, Trash } from "lucide-react";
import { call, type SkillDetail, type SkillSummary } from "../../lib/api";
import { useStore } from "../../lib/store";
import { ago, Badge, Dropdown } from "../../components/ui";
import { EditTab } from "./EditTab";
import { ReachTab } from "./ReachTab";
import { HistoryTab } from "./HistoryTab";
import { NotesTab, UsageTab } from "./UsageTab";
import { ConfirmModal, RenameModal } from "./modals";
import { plural, type DetailTab } from "./util";

export function ManagedDetail({ summary, tab, setTab }: { summary: SkillSummary; tab: DetailTab; setTab: (t: DetailTab) => void }) {
  const s = useStore();
  const [d, setD] = useState<SkillDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [rename, setRename] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);

  const load = useCallback(async () => {
    try {
      setD(await call<SkillDetail>("get_skill", { name: summary.name }));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, [summary.name]);

  useEffect(() => {
    load();
  }, [load, s.version]);

  if (error && !d) return <div className="det-loading faint">{error}</div>;
  if (!d) return <div className="det-loading" />;

  const openNotes = d.notes.filter((n) => !n.resolved).length;
  const errors = d.issues.filter((i) => i.severity === "error").length;
  const name = d.name;

  const copyPath = () => {
    navigator.clipboard?.writeText(d.path).then(() => s.toast("Path copied"));
  };

  const tabs: { id: DetailTab; label: string; badge?: number }[] = [
    { id: "edit", label: "Edit", badge: errors || undefined },
    { id: "reach", label: "Reach" },
    { id: "history", label: "History" },
    { id: "usage", label: "Usage" },
    { id: "notes", label: "Notes", badge: openNotes || undefined },
  ];

  return (
    <div className="det">
      <div className="lib-drag" data-tauri-drag-region />
      <header className="det-head">
        <div className="det-title-row">
          <h2 className="det-name selectable">{name}</h2>
          {d.meta.archived && <Badge>Archived</Badge>}
          {!d.deployed_here && !d.meta.archived && (
            <Badge tone="outline" title="Targeted at other machines only">
              Not on this machine
            </Badge>
          )}
          {d.meta.source?.kind === "git" && (
            <Badge tone="outline" title={d.meta.source.url}>
              From {d.meta.source.url.replace(/^https:\/\/github\.com\//, "").replace(/\.git$/, "")}
            </Badge>
          )}
          <span className="grow" />
          <button
            className={`btn ghost icon sm det-star${d.meta.starred ? " on" : ""}`}
            title={d.meta.starred ? "Unstar" : "Star"}
            onClick={() => s.run("set_starred", { name, starred: !d.meta.starred })}
          >
            <Star size={15} fill={d.meta.starred ? "currentColor" : "none"} />
          </button>
          <Dropdown
            trigger={(open) => (
              <button className="btn ghost icon sm" title="More" onClick={open}>
                <Ellipsis size={16} />
              </button>
            )}
          >
            {(close) => (
              <>
                <button
                  onClick={() => {
                    close();
                    setRename(true);
                  }}
                >
                  <Pencil size={13} /> Rename…
                </button>
                <button
                  onClick={() => {
                    close();
                    s.run("set_archived", { name, archived: !d.meta.archived }, d.meta.archived ? "Unarchived" : "Archived — removed from every agent");
                  }}
                >
                  {d.meta.archived ? <ArchiveRestore size={13} /> : <Archive size={13} />}
                  {d.meta.archived ? "Unarchive" : "Archive"}
                </button>
                <hr />
                <button
                  className="danger"
                  onClick={() => {
                    close();
                    setConfirmDelete(true);
                  }}
                >
                  <Trash size={13} /> Delete…
                </button>
              </>
            )}
          </Dropdown>
        </div>
        <p className={`det-desc selectable${d.description ? "" : " empty"}`}>{d.description || "No description, so agents can't discover this skill. Add one in the editor."}</p>
        <div className="det-facts">
          <span className="det-fact">
            <GitCommitVertical size={13} /> {plural(d.versions, "version")}
          </span>
          <span className="det-fact">
            <Clock size={12} /> Updated {ago(d.meta.head_ts)}
          </span>
          <span className="det-fact" title="Description (in every session's listing) + body (loaded when used)">
            <Coins size={12} /> {d.description_tokens} + {d.body_tokens} tokens
          </span>
          <span className="det-fact">
            <Activity size={12} /> {d.usage.recent}× in 30 days
          </span>
          <button className="det-fact det-path" title="Copy path" onClick={copyPath}>
            <FolderOpen size={12} /> {d.path}
          </button>
        </div>
      </header>

      <nav className="tabs">
        {tabs.map((t) => (
          <button key={t.id} className={`tab${tab === t.id ? " on" : ""}`} onClick={() => setTab(t.id)}>
            {t.label}
            {t.badge != null && <span className={`tab-badge${t.id === "edit" ? " err" : ""}`}>{t.badge}</span>}
          </button>
        ))}
      </nav>

      <div className="det-body">
        {tab === "edit" && <EditTab detail={d} reload={load} />}
        {tab === "reach" && <ReachTab detail={d} summary={summary} />}
        {tab === "history" && <HistoryTab detail={d} />}
        {tab === "usage" && <UsageTab detail={d} />}
        {tab === "notes" && <NotesTab detail={d} reload={load} />}
      </div>

      {rename && <RenameModal name={name} onClose={() => setRename(false)} />}
      {confirmDelete && (
        <ConfirmModal
          title={`Delete ${name}?`}
          body="It's removed from every agent on every synced machine. The full history is kept, so you can restore it later."
          confirm="Delete skill"
          onClose={() => setConfirmDelete(false)}
          onConfirm={async () => {
            const r = await s.run("delete_skill", { name }, `Deleted ${name}`);
            if (r !== undefined) s.select(null);
          }}
        />
      )}
    </div>
  );
}
