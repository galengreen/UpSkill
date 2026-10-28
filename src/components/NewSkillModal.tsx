import { useState } from "react";
import { BookOpen, File, Globe, ListChecks, Workflow } from "lucide-react";
import "../views/library.css";
import { useStore } from "../lib/store";
import { AgentChip, agentName, Modal } from "./ui";
import { emit, slugify, titleCase, validName } from "../views/library/util";

type Template = "blank" | "workflow" | "reference" | "checklist";

const TEMPLATES: { id: Template; label: string; sub: string; icon: typeof File }[] = [
  { id: "blank", label: "Blank", sub: "Just a heading", icon: File },
  { id: "workflow", label: "Workflow", sub: "Steps to follow", icon: Workflow },
  { id: "reference", label: "Reference", sub: "Facts to know", icon: BookOpen },
  { id: "checklist", label: "Checklist", sub: "Nothing skipped", icon: ListChecks },
];

function body(t: Template, name: string): string {
  const title = titleCase(name || "new-skill");
  switch (t) {
    case "workflow":
      return `# ${title}\n\n## Steps\n\n1. \n2. \n3. \n\n## Done when\n\n- \n`;
    case "reference":
      return `# ${title}\n\nThe facts the agent needs, kept short. Put long material in \`references/\` and link to it so it's only loaded when needed.\n\n## Essentials\n\n- \n`;
    case "checklist":
      return `# ${title}\n\nWork through every item. Don't skip any; say which ones don't apply and why.\n\n- [ ] \n- [ ] \n- [ ] \n`;
    default:
      return "";
  }
}

export function NewSkillModal() {
  const s = useStore();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [template, setTemplate] = useState<Template>("workflow");
  const [agents, setAgents] = useState<string[] | null>(null);
  const [groups, setGroups] = useState<string[]>(s.filter.group ? [s.filter.group] : []);
  const [busy, setBusy] = useState(false);

  const clean = name.replace(/-+$/, "");
  const taken = s.skills.some((x) => x.name === clean);
  const nameError = !clean ? null : !validName(clean) ? "Lowercase letters, numbers and single hyphens." : taken ? "That name is taken." : null;
  const len = [...description].length;
  const ok = !!clean && !nameError && description.trim().length > 0 && len <= 1024;
  const installed = s.agents.filter((a) => a.installed);
  const close = () => s.setNewSkill(false);

  const toggleAgent = (id: string) => {
    const cur = new Set(agents ?? installed.map((a) => a.id));
    if (cur.has(id)) cur.delete(id);
    else cur.add(id);
    setAgents(cur.size ? [...cur] : null);
  };

  const create = async () => {
    if (!ok || busy) return;
    setBusy(true);
    const r = await s.run("create_skill", { name: clean, description: description.trim(), body: body(template, clean), groups, ...(agents ? { agents } : {}) }, `Created ${clean}`);
    setBusy(false);
    if (r === undefined) return;
    close();
    s.openSkill(clean);
    emit("upskill:tab", "edit");
  };

  return (
    <Modal
      title="New skill"
      subtitle="Written once, placed for every agent you choose."
      onClose={close}
      width={560}
      footer={
        <>
          <span className="faint grow" style={{ fontSize: 11.5 }}>
            ⌘↵ to create
          </span>
          <button className="btn ghost" onClick={close}>
            Cancel
          </button>
          <button className="btn primary" disabled={!ok || busy} onClick={create}>
            {busy ? "Creating…" : "Create skill"}
          </button>
        </>
      }
    >
      <div
        className="col"
        style={{ gap: 16 }}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) create();
        }}
      >
        <div>
          <label className="label">Name</label>
          <input className="input mono" autoFocus placeholder="full-send" value={name} onChange={(e) => setName(slugify(e.target.value))} spellCheck={false} />
          {nameError ? <div className="field-err">{nameError}</div> : <div className="hint">Also the folder name and the /command people type.</div>}
        </div>
        <div>
          <label className="label row">
            Description
            <span className="grow" />
            <span className={`num trigger-count${len > 1024 ? " over" : len > 250 ? " long" : ""}`}>
              {len > 250 ? `${len}/1024` : `${len}/250`}
            </span>
          </label>
          <textarea
            className="textarea"
            rows={3}
            placeholder="Use when the user says “full send” or asks to ship a change end to end…"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
          />
          <div className="hint">This is a trigger, not a summary. Lead with when to use it and the exact words that should fire it.</div>
        </div>
        <div>
          <label className="label">Start from</label>
          <div className="tpl-grid">
            {TEMPLATES.map((t) => (
              <button key={t.id} className={`tpl${template === t.id ? " on" : ""}`} onClick={() => setTemplate(t.id)}>
                <t.icon size={15} />
                <span className="tpl-title">{t.label}</span>
                <span className="tpl-sub">{t.sub}</span>
              </button>
            ))}
          </div>
        </div>
        <div>
          <label className="label">Agents</label>
          <div className="pills">
            <button className={`pill${agents === null ? " on" : ""}`} onClick={() => setAgents(null)}>
              <Globe size={14} /> Everywhere
            </button>
            {installed.map((a) => {
              const on = agents === null || agents.includes(a.id);
              return (
                <button key={a.id} className={`pill${on && agents !== null ? " on" : ""}`} onClick={() => toggleAgent(a.id)}>
                  <AgentChip id={a.id} off={!on} /> {agentName(a.id)}
                </button>
              );
            })}
          </div>
        </div>
        {s.groups.length > 0 && (
          <div>
            <label className="label">Groups</label>
            <div className="pills">
              {s.groups.map((g) => (
                <button key={g.id} className={`pill${groups.includes(g.id) ? " on" : ""}`} onClick={() => setGroups((cur) => (cur.includes(g.id) ? cur.filter((x) => x !== g.id) : [...cur, g.id]))}>
                  <span className="dot" style={{ background: g.color }} /> {g.name}
                </button>
              ))}
            </div>
          </div>
        )}
      </div>
    </Modal>
  );
}
