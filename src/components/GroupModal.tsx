import { useState } from "react";
import { Globe } from "lucide-react";
import "../views/library.css";
import type { Group } from "../lib/api";
import { useStore } from "../lib/store";
import { AGENT_IDS, AgentChip, agentName, Modal } from "./ui";

const COLOURS = ["#b4f461", "#7cc7ff", "#ff9f6b", "#c49bff", "#ffd166", "#5eead4", "#ff7aa2", "#a1a1aa"];

/** Create a group, or edit/delete one when `group` is given. */
export function GroupModal({ group, onClose }: { group: Group | null; onClose: () => void }) {
  const s = useStore();
  const [name, setName] = useState(group?.name ?? "");
  const [description, setDescription] = useState(group?.description ?? "");
  const [color, setColor] = useState(group?.color ?? COLOURS[s.groups.length % COLOURS.length]);
  const [agents, setAgents] = useState<string[] | null>(group?.agents ?? null);
  const [armed, setArmed] = useState(false);
  const installed = new Set(s.agents.filter((a) => a.installed).map((a) => a.id));
  const ok = name.trim().length > 0;

  const toggle = (id: string) => {
    const cur = new Set(agents ?? []);
    if (cur.has(id)) cur.delete(id);
    else cur.add(id);
    setAgents(cur.size ? [...cur] : null);
  };

  const save = async () => {
    if (!ok) return;
    if (group) {
      const r = await s.run("update_group", { group: group.id, name: name.trim(), description, color, agents: agents ?? "inherit" }, "Group saved");
      if (r !== undefined) onClose();
    } else {
      const r = await s.run<{ id: string }>("create_group", { name: name.trim(), description, color, ...(agents ? { agents } : {}) }, `Created ${name.trim()}`);
      if (r) {
        s.go("library");
        s.setFilter({ group: r.id, kind: "all" });
        onClose();
      }
    }
  };

  const remove = async () => {
    if (!group) return;
    if (!armed) {
      setArmed(true);
      return;
    }
    const r = await s.run("delete_group", { group: group.id }, `Deleted ${group.name}. Its skills are untouched`);
    if (r !== undefined) {
      s.setFilter({ group: null });
      onClose();
    }
  };

  return (
    <Modal
      title={group ? "Edit group" : "New group"}
      subtitle="Groups organise skills and can decide which agents their members reach."
      onClose={onClose}
      footer={
        <>
          {group && (
            <button className="btn danger" onClick={remove}>
              {armed ? "Click again to delete" : "Delete group"}
            </button>
          )}
          <span className="grow" />
          <button className="btn ghost" onClick={onClose}>
            Cancel
          </button>
          <button className="btn primary" disabled={!ok} onClick={save}>
            {group ? "Save" : "Create group"}
          </button>
        </>
      }
    >
      <div className="row" style={{ gap: 12, alignItems: "flex-end" }}>
        <div className="grow">
          <label className="label">Name</label>
          <input className="input" autoFocus placeholder="Shipping" value={name} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && save()} />
        </div>
        <div className="swatches">
          {COLOURS.map((c) => (
            <button key={c} className={`swatch${c === color ? " on" : ""}`} style={{ background: c, color: c }} onClick={() => setColor(c)} aria-label={c} />
          ))}
        </div>
      </div>
      <div>
        <label className="label">Description</label>
        <input className="input" placeholder="What belongs here" value={description} onChange={(e) => setDescription(e.target.value)} />
      </div>
      <div>
        <label className="label">Agents</label>
        <div className="pills">
          <button className={`pill${agents === null ? " on" : ""}`} onClick={() => setAgents(null)}>
            <Globe size={14} /> Don't restrict
          </button>
          {AGENT_IDS.filter((id) => installed.has(id) || agents?.includes(id)).map((id) => (
            <button key={id} className={`pill${agents?.includes(id) ? " on" : ""}`} onClick={() => toggle(id)}>
              <AgentChip id={id} off={!agents?.includes(id)} /> {agentName(id)}
            </button>
          ))}
        </div>
        <div className="hint">Members without their own targets only go to these agents.</div>
      </div>
    </Modal>
  );
}
