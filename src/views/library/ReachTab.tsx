import { useEffect, useState } from "react";
import { ArrowRight, CircleSlash, Copy, Globe, Laptop, Link2, MapPin, Plus, TriangleAlert } from "lucide-react";
import { call, type MachineInfo, type SkillDetail, type SkillSummary } from "../../lib/api";
import { useStore } from "../../lib/store";
import { AGENT_IDS, AgentChip, AgentStack, agentName } from "../../components/ui";
import { emit } from "./util";

export function ReachTab({ detail, summary }: { detail: SkillDetail; summary: SkillSummary }) {
  const s = useStore();
  const name = detail.name;
  const [machines, setMachines] = useState<MachineInfo[]>([]);

  useEffect(() => {
    call<MachineInfo[]>("machines").then(setMachines).catch(() => {});
  }, [s.version]);

  const installed = new Set(s.agents.filter((a) => a.installed).map((a) => a.id));
  const explicit = detail.meta.agents;
  const everywhere = detail.agents.includes("*");
  const effective = everywhere ? AGENT_IDS : detail.agents;

  const toggleAgent = (id: string) => {
    const cur = new Set(explicit ?? effective);
    if (cur.has(id)) cur.delete(id);
    else cur.add(id);
    if (cur.size === 0) {
      s.toast("A skill needs at least one agent. Archive it instead.", "error");
      return;
    }
    s.run("set_skill_targets", { name, agents: [...cur] });
  };

  const machineIds = machines.map((m) => m.id);
  const effMachines = summary.machines;
  const toggleMachine = (id: string) => {
    const cur = new Set(detail.meta.machines ?? effMachines ?? machineIds);
    if (cur.has(id)) cur.delete(id);
    else cur.add(id);
    if (cur.size === 0) {
      s.toast("Pick at least one machine, or archive the skill.", "error");
      return;
    }
    s.run("set_skill_targets", { name, machines: [...cur] });
  };

  const toggleGroup = (id: string) => {
    const cur = new Set(detail.meta.groups);
    if (cur.has(id)) cur.delete(id);
    else cur.add(id);
    s.run("set_skill_groups", { name, groups: [...cur] });
  };

  // Which folders each agent reads, keyed by "~/.x/skills".
  const readersOf = (dir: string) => {
    const path = `~/${dir}`;
    for (const a of s.agents) for (const d of a.dirs) if (d.path === path) return d.readers;
    return [];
  };
  const dirFor = (agent: string) => detail.placement.dirs.find((d) => readersOf(d).includes(agent));
  const p = detail.placement;
  const machineName = s.status?.machine.name ?? "this machine";
  const agentHint =
    explicit !== null
      ? "Set on this skill. Agents that aren't installed here still get it on machines where they are."
      : everywhere
        ? "Everywhere, the default. Switch an agent off to narrow it down."
        : "Inherited from its groups.";

  return (
    <div className="det-scroll">
      <div className="reach">
        <section className="reach-sec">
          <h4>Agents</h4>
          <p className="hint">{agentHint}</p>
          <div className="pills">
            <button className={`pill${explicit === null ? " on" : ""}`} onClick={() => explicit !== null && s.run("set_skill_targets", { name, agents: "inherit" })}>
              <Globe size={14} /> {explicit === null && !everywhere ? "Inherit from groups" : "Everywhere"}
            </button>
            <span className="pill-sep" />
            {AGENT_IDS.map((id) => {
              const on = effective.includes(id);
              return (
                <button
                  key={id}
                  className={`pill${on ? " on" : ""}${explicit === null && on ? " inherited" : ""}${installed.has(id) ? "" : " disabled"}`}
                  title={installed.has(id) ? undefined : `${agentName(id)} isn't installed on this machine`}
                  onClick={() => toggleAgent(id)}
                >
                  <AgentChip id={id} off={!on} /> {agentName(id)}
                </button>
              );
            })}
          </div>
        </section>

        <section className="reach-sec">
          <h4>Machines</h4>
          <p className="hint">{detail.meta.machines ? "Only these machines get it." : effMachines ? "Inherited from its groups." : "Every machine that shares your library."}</p>
          <div className="pills">
            <button className={`pill${!detail.meta.machines && !effMachines ? " on" : ""}`} onClick={() => detail.meta.machines && s.run("set_skill_targets", { name, machines: "inherit" })}>
              <Globe size={14} /> All machines
            </button>
            <span className="pill-sep" />
            {machines.map((m) => {
              const on = (detail.meta.machines ?? effMachines ?? machineIds).some((x) => x === m.id || x === m.name);
              return (
                <button key={m.id} className={`pill${on ? " on" : ""}${!detail.meta.machines && on ? " inherited" : ""}`} onClick={() => toggleMachine(m.id)}>
                  <Laptop size={14} /> {m.name}
                  {m.id === s.status?.machine.id && <span className="faint">this one</span>}
                </button>
              );
            })}
          </div>
        </section>

        <section className="reach-sec">
          <h4>Groups</h4>
          <p className="hint">Groups can carry their own agent and machine targets, which members inherit.</p>
          <div className="pills">
            {s.groups.map((g) => (
              <button key={g.id} className={`pill${detail.meta.groups.includes(g.id) ? " on" : ""}`} onClick={() => toggleGroup(g.id)}>
                <span className="dot" style={{ background: g.color }} /> {g.name}
              </button>
            ))}
            <button className="pill ghost-pill" onClick={() => emit("upskill:new-group")}>
              <Plus size={13} /> New group
            </button>
          </div>
        </section>

        <section className="card card-pad">
          <div className="card-title">
            <MapPin size={13} /> Where it lands on {machineName}
          </div>
          {!detail.deployed_here ? (
            <p className="reach-empty">{detail.meta.archived ? "Archived, so it isn't placed anywhere." : "Not targeted at this machine, so nothing is placed here."}</p>
          ) : p.dirs.length === 0 ? (
            <p className="reach-empty">None of its agents are installed here.</p>
          ) : (
            p.dirs.map((d) => (
              <div key={d} className="reach-dir">
                <Link2 size={13} className="faint" />
                <span className="mono reach-path selectable">
                  ~/{d}/<b>{name}</b>
                </span>
                <span className="grow" />
                <AgentStack agents={readersOf(d).filter((r) => installed.has(r))} max={8} />
              </div>
            ))
          )}
          {p.leaks.map((a) => (
            <div key={a} className="reach-note warn">
              <TriangleAlert size={13} />
              <span>
                <b>{agentName(a)}</b> will also see this, because it reads <span className="mono">~/{dirFor(a) ?? "…"}</span>. There's no folder only your chosen agents read.
              </span>
            </div>
          ))}
          {p.duplicates.map((a) => (
            <div key={a} className="reach-note">
              <Copy size={13} />
              <span>
                <b>{agentName(a)}</b> sees it from more than one folder. Harmless: the first match wins.
              </span>
            </div>
          ))}
          {p.unreachable.length > 0 && (
            <div className="reach-note muted-note">
              <CircleSlash size={13} />
              <span>
                Not installed here: {p.unreachable.map(agentName).join(", ")}. They'll get it on machines where they're installed.
              </span>
            </div>
          )}
        </section>

        {(detail.deps.length > 0 || detail.dependents.length > 0 || detail.overlaps.length > 0) && (
          <section className="card card-pad reach-links">
            {detail.deps.length > 0 && (
              <div className="reach-link-row">
                <span className="reach-link-label">Calls</span>
                <div className="pills">
                  {detail.deps.map((n) => (
                    <SkillLink key={n} name={n} />
                  ))}
                </div>
                <span className="hint">Follows this skill to every agent it reaches.</span>
              </div>
            )}
            {detail.dependents.length > 0 && (
              <div className="reach-link-row">
                <span className="reach-link-label">Called by</span>
                <div className="pills">
                  {detail.dependents.map((n) => (
                    <SkillLink key={n} name={n} />
                  ))}
                </div>
              </div>
            )}
            {detail.overlaps.length > 0 && (
              <div className="reach-link-row">
                <span className="reach-link-label">Competes with</span>
                <div className="pills">
                  {detail.overlaps.map(([n, sim]) => (
                    <SkillLink key={n} name={n} extra={`${Math.round(sim * 100)}%`} warn />
                  ))}
                </div>
                <span className="hint">Similar descriptions make agents pick between them at random. Merge them or sharpen the triggers.</span>
              </div>
            )}
          </section>
        )}
      </div>
    </div>
  );
}

function SkillLink({ name, extra, warn }: { name: string; extra?: string; warn?: boolean }) {
  const s = useStore();
  return (
    <button className={`skill-link${warn ? " warn" : ""}`} onClick={() => s.openSkill(name)}>
      {name}
      {extra && <span className="skill-link-extra">{extra}</span>}
      <ArrowRight size={11} />
    </button>
  );
}
