import { useState } from "react";
import { FolderOpen, Inbox, Lock, Package, Puzzle, Link2, Boxes } from "lucide-react";
import type { SkillSummary } from "../../lib/api";
import { useStore } from "../../lib/store";
import { AgentStack, Badge } from "../../components/ui";
import { ByAgent, UsageBars, UsageTiles } from "./UsageTab";
import { KIND_LABEL } from "./util";

const EXPLAIN: Record<string, (s: SkillSummary) => { icon: typeof Inbox; title: string; body: string }> = {
  unmanaged: () => ({
    icon: Inbox,
    title: "Living loose in an agent folder",
    body: "No history, no sync and no control over which agents see it. Adopting moves it into your library (the original is backed up) and links it straight back to the same agents.",
  }),
  link: () => ({
    icon: Link2,
    title: "A symlink to somewhere outside UpSkill",
    body: "Another tool or script manages this folder. Adopt it to bring the content under version history and sync. The original is backed up first.",
  }),
  system: (s) => ({
    icon: Lock,
    title: `Installed by ${s.source ?? "the agent"}`,
    body: "The agent manages this one and may overwrite changes, so UpSkill leaves it alone. Its usage is still tracked, and it still counts against the listing budget.",
  }),
  plugin: (s) => ({
    icon: Puzzle,
    title: `Part of the ${s.source ?? ""} plugin`,
    body: "Plugins ship and update their own skills. Manage it with Claude Code's /plugin command. UpSkill tracks its usage so you can tell whether the plugin earns its keep.",
  }),
  builtin: () => ({
    icon: Package,
    title: "Bundled inside the agent",
    body: "There's no file on disk to manage, but UpSkill sees it in session logs, so its usage is still counted.",
  }),
};

export function ExternalDetail({ skill }: { skill: SkillSummary }) {
  const s = useStore();
  const [busy, setBusy] = useState(false);
  const info = (EXPLAIN[skill.kind] ?? EXPLAIN.builtin)(skill);
  const Icon = info.icon;
  const adoptable = skill.kind === "unmanaged" || skill.kind === "link";

  const adopt = async () => {
    setBusy(true);
    const r = await s.run("adopt", { name: skill.name }, `Adopted ${skill.name}`);
    setBusy(false);
    if (r !== undefined) s.select(skill.name);
  };

  return (
    <div className="det">
      <div className="lib-drag" data-tauri-drag-region />
      <header className="det-head">
        <div className="det-title-row">
          <h2 className="det-name selectable">{skill.name}</h2>
          <Badge tone="outline">{KIND_LABEL[skill.kind] ?? skill.kind}</Badge>
        </div>
        <p className={`det-desc selectable${skill.description ? "" : " empty"}`}>{skill.description || "No description."}</p>
      </header>
      <div className="det-scroll" style={{ paddingTop: 4 }}>
        <div className="ext">
          <section className={`card card-pad ext-card${adoptable ? " adoptable" : ""}`}>
            <div className="ext-icon">
              <Icon size={17} />
            </div>
            <div className="grow">
              <div className="ext-title">{info.title}</div>
              <p className="ext-body">{info.body}</p>
              <div className="ext-facts">
                {skill.path && (
                  <span className="det-fact mono selectable">
                    <FolderOpen size={12} /> {skill.path}
                  </span>
                )}
                <span className="det-fact">
                  <Boxes size={12} /> Seen by <AgentStack agents={skill.agents} max={8} />
                </span>
              </div>
              {adoptable && (
                <button className="btn primary" style={{ marginTop: 14 }} disabled={busy} onClick={adopt}>
                  {busy ? "Adopting…" : "Adopt into library"}
                </button>
              )}
            </div>
          </section>
          {skill.usage.total === 0 ? (
            <p className="ext-unused">No agent has used it yet. If that stays true, it's only costing listing budget.</p>
          ) : (
            <>
              <UsageTiles u={skill.usage} />
              <section className="card card-pad">
                <div className="card-title">Daily uses</div>
                <UsageBars spark={skill.usage.spark} />
              </section>
              <section className="card card-pad">
                <div className="card-title">By agent, all time</div>
                <ByAgent u={skill.usage} />
              </section>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
