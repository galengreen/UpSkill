import { useEffect, useMemo, useRef, useState } from "react";
import { Archive, ArrowDownWideNarrow, Check, ChevronsUp, Download, Inbox, MessageSquare, Pencil, Plus, Search, Star, X } from "lucide-react";
import "./library.css";
import { useStore } from "../lib/store";
import type { Group, SkillSummary } from "../lib/api";
import { Dropdown, Empty, Kbd, Segmented, Sev, Sparkline } from "../components/ui";
import { GroupModal } from "../components/GroupModal";
import { ManagedDetail } from "./library/Detail";
import { ExternalDetail } from "./library/ExternalDetail";
import { ImportRepoModal } from "./library/modals";
import { AgentDots } from "./library/AgentDots";
import { filterSkills, KIND_LABEL, plural, type DetailTab, type Sort } from "./library/util";


const SORTS: { value: Sort; label: string }[] = [
  { value: "used", label: "Most used" },
  { value: "recent", label: "Recently changed" },
  { value: "az", label: "A–Z" },
];

export function Library() {
  const s = useStore();
  const [sort, setSortState] = useState<Sort>(() => (localStorage.getItem("upskill.sort") as Sort) || "used");
  const [tab, setTab] = useState<DetailTab>("edit");
  const [importOpen, setImportOpen] = useState(false);
  const [groupModal, setGroupModal] = useState<Group | "new" | null>(null);
  const rowsRef = useRef<HTMLDivElement>(null);

  const items = useMemo(() => filterSkills(s.skills, s.filter, sort), [s.skills, s.filter, sort]);
  const group = s.groups.find((g) => g.id === s.filter.group) ?? null;
  const current = s.skills.find((x) => x.name === s.selected) ?? null;
  const setSort = (v: Sort) => {
    localStorage.setItem("upskill.sort", v);
    setSortState(v);
  };

  useEffect(() => {
    const onImport = () => setImportOpen(true);
    const onGroup = () => setGroupModal("new");
    const onTab = (e: Event) => setTab((e as CustomEvent<DetailTab>).detail);
    window.addEventListener("upskill:import-repo", onImport);
    window.addEventListener("upskill:new-group", onGroup);
    window.addEventListener("upskill:tab", onTab);
    return () => {
      window.removeEventListener("upskill:import-repo", onImport);
      window.removeEventListener("upskill:new-group", onGroup);
      window.removeEventListener("upskill:tab", onTab);
    };
  }, []);

  // ↑/↓ walks the list whenever focus isn't in a text field or the editor.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      const t = e.target as HTMLElement;
      if (t.closest("input, textarea, [contenteditable=true], .cm-editor") || document.querySelector(".overlay")) return;
      e.preventDefault();
      const i = items.findIndex((x) => x.name === s.selected);
      const n = e.key === "ArrowDown" ? Math.min(items.length - 1, i + 1) : Math.max(0, i < 0 ? 0 : i - 1);
      const next = items[n];
      if (!next) return;
      s.select(next.name);
      rowsRef.current?.querySelector(`[data-name="${CSS.escape(next.name)}"]`)?.scrollIntoView({ block: "nearest" });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [items, s]);

  const unmanaged = s.status?.unmanaged ?? 0;
  const showBanner = unmanaged > 0 && !s.filter.group && (s.filter.kind === "all" || s.filter.kind === "unmanaged") && !s.filter.query;
  const libraryEmpty = s.loaded && s.skills.length === 0;

  return (
    <div className="lib">
      <section className="lib-list">
        <div className="lib-drag" data-tauri-drag-region />
        <header className="lib-list-head">
          <div className="lib-title-row">
            {group && <span className="dot" style={{ background: group.color }} />}
            <h1 className="lib-title ellipsis">{group?.name ?? "Library"}</h1>
            <span className="faint num lib-title-count">{items.length}</span>
            <div className="lib-head-actions">
              {group && (
                <button className="btn ghost icon sm" title="Edit group" onClick={() => setGroupModal(group)}>
                  <Pencil size={13} />
                </button>
              )}
              <Dropdown
                trigger={(open) => (
                  <button className="btn ghost icon sm" title={`Sort: ${SORTS.find((x) => x.value === sort)?.label}`} onClick={open}>
                    <ArrowDownWideNarrow size={14} />
                  </button>
                )}
              >
                {(close) =>
                  SORTS.map((o) => (
                    <button
                      key={o.value}
                      onClick={() => {
                        setSort(o.value);
                        close();
                      }}
                    >
                      <span className="grow">{o.label}</span>
                      {sort === o.value && <Check size={13} />}
                    </button>
                  ))
                }
              </Dropdown>
              <button className="btn ghost icon sm" title="Install from a repo" onClick={() => setImportOpen(true)}>
                <Download size={14} />
              </button>
              <button className="btn icon sm" title="New skill (⌘N)" onClick={() => s.setNewSkill(true)}>
                <Plus size={14} />
              </button>
            </div>
          </div>
          <div className="lib-search">
            <Search size={13} />
            <input className="input" placeholder="Filter skills" value={s.filter.query} onChange={(e) => s.setFilter({ query: e.target.value })} spellCheck={false} />
            {s.filter.query && (
              <button className="lib-search-clear" onClick={() => s.setFilter({ query: "" })} aria-label="Clear">
                <X size={12} />
              </button>
            )}
          </div>
          <div className="lib-seg">
            <Segmented
              value={s.filter.kind}
              onChange={(kind) => s.setFilter({ kind })}
              options={[
                { value: "all", label: "All" },
                { value: "managed", label: "Managed" },
                { value: "unmanaged", label: "Unmanaged" },
                { value: "external", label: "External" },
                { value: "starred", label: <Star size={12} aria-label="Starred" /> },
                { value: "archived", label: <Archive size={12} aria-label="Archived" /> },
              ]}
            />
          </div>
        </header>

        <div className="lib-rows" ref={rowsRef}>
          {showBanner && (
            <div className="lib-banner">
              <Inbox size={15} />
              <div className="grow">
                <div className="lib-banner-title">{plural(unmanaged, "unmanaged skill")} found</div>
                <div className="lib-banner-sub">Bring them in for history, sync and placement. Originals are backed up.</div>
              </div>
              <button className="btn sm primary" onClick={() => s.run("adopt_all", {}, "Adopted every unmanaged skill")}>
                Adopt all
              </button>
            </div>
          )}
          {items.map((sk) => (
            <Row key={sk.name} sk={sk} on={sk.name === s.selected} showKind={s.filter.kind !== "unmanaged"} onClick={() => s.select(sk.name)} />
          ))}
          {s.loaded && items.length === 0 && !libraryEmpty && (
            <Empty icon={<Search size={18} />} title="Nothing here">
              {s.filter.query ? "No skills match that filter." : s.filter.kind === "archived" ? "Archived skills stay in your library but aren't given to any agent." : "No skills in this view yet."}
            </Empty>
          )}
        </div>
      </section>

      <section className="lib-detail">
        {current ? (
          current.kind === "managed" ? (
            <ManagedDetail key={current.name} summary={current} tab={tab} setTab={setTab} />
          ) : (
            <ExternalDetail key={current.name} skill={current} />
          )
        ) : (
          <>
            <div className="lib-drag" data-tauri-drag-region />
            {libraryEmpty ? (
              <Empty
                icon={<ChevronsUp size={20} />}
                title="Your library is empty"
                action={
                  <div className="row" style={{ justifyContent: "center" }}>
                    <button className="btn primary" onClick={() => s.setNewSkill(true)}>
                      <Plus size={14} /> New skill
                    </button>
                    <button className="btn" onClick={() => setImportOpen(true)}>
                      <Download size={14} /> Install from a repo
                    </button>
                  </div>
                }
              >
                One library for every agent. Write a skill once and UpSkill puts it where Claude Code, Codex, Amp and the rest will find it.
              </Empty>
            ) : (
              <Empty icon={<ChevronsUp size={20} />} title="Pick a skill">
                Choose one on the left, or press <Kbd>⌘N</Kbd> to write a new one. <Kbd>⌘K</Kbd> searches everything.
              </Empty>
            )}
          </>
        )}
      </section>

      {importOpen && <ImportRepoModal onClose={() => setImportOpen(false)} />}
      {groupModal && <GroupModal group={groupModal === "new" ? null : groupModal} onClose={() => setGroupModal(null)} />}
    </div>
  );
}

function Row({ sk, on, showKind, onClick }: { sk: SkillSummary; on: boolean; showKind: boolean; onClick: () => void }) {
  return (
    <button data-name={sk.name} className={`lib-row${on ? " on" : ""}${sk.archived ? " dim" : ""}`} onClick={onClick}>
      <div className="lib-row-top">
        <span className="lib-row-name">{sk.name}</span>
        {sk.starred && <Star size={11} className="lib-star" fill="currentColor" />}
        {sk.errors > 0 ? <Sev s="error" /> : sk.warnings > 0 ? <Sev s="warn" /> : null}
        {sk.open_notes > 0 && <MessageSquare size={11} className="faint" />}
        {sk.kind !== "managed" && showKind && <span className="lib-kind">{KIND_LABEL[sk.kind] ?? sk.kind}</span>}
        <span className="grow" />
        {sk.usage.spark.length > 0 ? <Sparkline data={sk.usage.spark} width={48} height={14} /> : <span style={{ width: 48 }} />}
        <span className="lib-row-count num">{sk.usage.recent > 0 ? sk.usage.recent : ""}</span>
      </div>
      <div className="lib-row-bottom">
        <span className="lib-row-desc ellipsis">{sk.description || <i>No description</i>}</span>
        <AgentDots agents={sk.agents} />
      </div>
    </button>
  );
}

