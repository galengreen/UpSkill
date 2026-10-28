import { useState } from "react";
import { Check, Send } from "lucide-react";
import type { SkillDetail, SkillUsage, UsageEvent } from "../../lib/api";
import { useStore } from "../../lib/store";
import { AGENT_META, AgentChip, ago, agentName, authorLabel, Empty, fmtDate } from "../../components/ui";
import { shortPath } from "./util";

export function UsageBars({ spark }: { spark: number[] }) {
  const data = spark.length ? spark : Array(30).fill(0);
  const max = Math.max(1, ...data);
  const empty = data.every((v) => v === 0);
  return (
    <div className="bars-wrap">
      <div className="bars">
        {data.map((v, i) => (
          <div key={i} className={`bar${v === 0 ? " zero" : ""}`} style={{ height: v === 0 ? 2 : `${Math.max(6, (v / max) * 100)}%` }} title={`${v} use${v === 1 ? "" : "s"}, ${data.length - 1 - i === 0 ? "today" : `${data.length - 1 - i}d ago`}`} />
        ))}
        {empty && <div className="bars-empty">No uses in the last 30 days</div>}
      </div>
      <div className="bars-axis">
        <span>30 days ago</span>
        <span>Today</span>
      </div>
    </div>
  );
}

export function UsageTiles({ u }: { u: SkillUsage }) {
  const auto = Math.max(0, u.total - u.manual);
  return (
    <div className="tiles">
      <Tile label="Last 30 days" value={u.recent} />
      <Tile label="All time" value={u.total} />
      <Tile label="Auto / typed" value={`${auto} / ${u.manual}`} sub={u.total ? `${Math.round((auto / u.total) * 100)}% picked by the agent` : undefined} />
      <Tile label="Last used" value={u.last_ts ? ago(u.last_ts) : "Never"} />
    </div>
  );
}

function Tile({ label, value, sub }: { label: string; value: string | number; sub?: string }) {
  return (
    <div className="tile">
      <div className="tile-label">{label}</div>
      <div className="tile-value num">{value}</div>
      {sub && <div className="tile-sub">{sub}</div>}
    </div>
  );
}

export function ByAgent({ u }: { u: SkillUsage }) {
  const rows = Object.entries(u.by_agent).sort((a, b) => b[1] - a[1]);
  const max = Math.max(1, ...rows.map((r) => r[1]));
  if (!rows.length) return <p className="faint" style={{ margin: 0 }}>No agent has used it yet.</p>;
  return (
    <div>
      {rows.map(([a, n]) => (
        <div key={a} className="agent-bar-row">
          <span className="row">
            <AgentChip id={a} /> {agentName(a)}
          </span>
          <div className="agent-bar-track">
            <div className="agent-bar" style={{ width: `${(n / max) * 100}%`, background: AGENT_META[a]?.color ?? "var(--text-3)" }} />
          </div>
          <span className="num faint" style={{ textAlign: "right" }}>
            {n}
          </span>
        </div>
      ))}
    </div>
  );
}

const MODE: Record<string, string> = { auto: "Auto", manual: "Typed", reported: "Reported" };

export function RecentEvents({ events }: { events: UsageEvent[] }) {
  if (!events.length) return <p className="faint" style={{ margin: 0 }}>Nothing recorded yet. UpSkill reads Claude Code and Codex session logs as you work.</p>;
  return (
    <div>
      {events.map((e, i) => (
        <div key={i} className="event-row">
          <AgentChip id={e.agent} />
          <span className={`event-mode ${e.mode}`}>{MODE[e.mode] ?? e.mode}</span>
          <span className="mono faint ellipsis grow" title={e.cwd}>
            {e.cwd ? shortPath(e.cwd) : ""}
          </span>
          <span className="faint" title={fmtDate(e.ts)}>
            {ago(e.ts)}
          </span>
        </div>
      ))}
    </div>
  );
}

export function UsageTab({ detail }: { detail: SkillDetail }) {
  const u = detail.usage;
  return (
    <div className="det-scroll">
      <div className="usage">
        <UsageTiles u={u} />
        <section className="card card-pad">
          <div className="card-title">Daily uses</div>
          <UsageBars spark={u.spark} />
        </section>
        <div className="usage-cols">
          <section className="card card-pad">
            <div className="card-title">By agent, all time</div>
            <ByAgent u={u} />
          </section>
          <section className="card card-pad">
            <div className="card-title">Recent</div>
            <RecentEvents events={detail.recent.slice(0, 12)} />
          </section>
        </div>
      </div>
    </div>
  );
}

export function NotesTab({ detail, reload }: { detail: SkillDetail; reload: () => void }) {
  const s = useStore();
  const [text, setText] = useState("");
  const [showResolved, setShowResolved] = useState(false);
  const open = detail.notes.filter((n) => !n.resolved).sort((a, b) => b.ts - a.ts);
  const resolved = detail.notes.filter((n) => n.resolved).sort((a, b) => b.ts - a.ts);

  const add = async () => {
    const t = text.trim();
    if (!t) return;
    const r = await s.run("add_note", { skill: detail.name, text: t });
    if (r !== undefined) {
      setText("");
      reload();
    }
  };
  const toggle = async (id: string, resolved: boolean) => {
    await s.run("resolve_note", { id, resolved });
    reload();
  };

  const NoteRow = ({ n }: { n: SkillDetail["notes"][number] }) => (
    <div className={`note${n.resolved ? " resolved" : ""}`}>
      <button className={`note-check${n.resolved ? " done" : ""}`} title={n.resolved ? "Reopen" : "Resolve"} onClick={() => toggle(n.id, !n.resolved)}>
        {n.resolved && <Check size={10} strokeWidth={3} />}
      </button>
      <div className="grow">
        <div className="note-text selectable">{n.text}</div>
        <div className="note-meta">
          {authorLabel(n.author).label} · {ago(n.ts)}
        </div>
      </div>
    </div>
  );

  return (
    <div className="det-scroll">
      <div className="notes">
        <div className="note-compose">
          <textarea
            className="textarea"
            rows={2}
            placeholder="What went wrong, or what should change? Agents can leave notes here too."
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) add();
            }}
          />
          <button className="btn primary sm" disabled={!text.trim()} onClick={add}>
            <Send size={12} /> Add note
          </button>
        </div>
        {open.length === 0 && resolved.length === 0 && <Empty title="No notes">When a skill misfires, jot it down here. Notes sync to your other machines and show up in Health.</Empty>}
        {open.map((n) => (
          <NoteRow key={n.id} n={n} />
        ))}
        {resolved.length > 0 && (
          <button className="btn ghost sm" style={{ marginTop: 12 }} onClick={() => setShowResolved((v) => !v)}>
            {showResolved ? "Hide" : "Show"} {resolved.length} resolved
          </button>
        )}
        {showResolved && resolved.map((n) => <NoteRow key={n.id} n={n} />)}
      </div>
    </div>
  );
}
