import { useState } from "react";
import { Check, FolderOpen, Plug } from "lucide-react";
import "./pages.css";
import type { AgentInfo } from "../lib/api";
import { useStore } from "../lib/store";
import { AGENT_META, AgentChip, agentName, Badge, compact } from "../components/ui";

export function Agents() {
  const { agents, status } = useStore();
  const installed = agents.filter((a) => a.installed);
  const absent = agents.filter((a) => !a.installed);

  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">Agents</h1>
            <div className="page-sub">
              {installed.length} agent{installed.length === 1 ? "" : "s"} found on {status?.machine.name ?? "this machine"}. UpSkill keeps every one of them in step.
            </div>
          </div>
        </div>

        <div className="card card-pad placement">
          <div>
            <div style={{ fontSize: 14, fontWeight: 600, letterSpacing: "-0.01em" }}>How placement works</div>
            <p>
              Every skill lives once in your library. UpSkill symlinks it into the folders each agent reads, choosing the fewest folders that reach the agents you picked,
              and never a folder that would leak it to an agent you didn't. Agents that edit a skill edit the library copy, so the change is versioned and goes everywhere.
            </p>
          </div>
          <PlacementDiagram agents={installed.map((a) => a.id)} />
        </div>

        <div className="pg-grid cols-2" style={{ marginTop: 14 }}>
          {installed.map((a) => (
            <AgentCard key={a.id} a={a} />
          ))}
        </div>

        {absent.length > 0 && (
          <div className="pg-section">
            <div className="pg-section-head">
              <h2>Not found here</h2>
              <p>Install any of these and UpSkill picks them up automatically.</p>
            </div>
            <div className="pg-grid cols-3">
              {absent.map((a) => (
                <div key={a.id} className="card agent-card absent">
                  <div className="agent-head">
                    <AgentChip id={a.id} size={24} />
                    <h3 style={{ fontSize: 13 }}>{a.name}</h3>
                    <span className="faint mono" style={{ marginLeft: "auto", fontSize: 11 }}>
                      {a.dirs[a.dirs.length - 1]?.path}
                    </span>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function AgentCard({ a }: { a: AgentInfo }) {
  const { run, toast } = useStore();
  const [busy, setBusy] = useState(false);
  const connect = async () => {
    setBusy(true);
    const out = await run<{ message: string }>("mcp_install", { agent: a.id });
    if (out?.message) toast(out.message);
    setBusy(false);
  };
  return (
    <div className="card agent-card">
      <div className="agent-head">
        <AgentChip id={a.id} size={32} />
        <div className="grow">
          <h3>{a.name}</h3>
          <div className="faint" style={{ fontSize: 11.5 }}>
            {a.dirs.length} folder{a.dirs.length === 1 ? "" : "s"} read
          </div>
        </div>
        <Badge tone="accent">Installed</Badge>
      </div>

      <div className="agent-counts">
        <div>
          <b>{a.managed}</b>
          <span>managed</span>
        </div>
        <div>
          <b style={{ color: a.unmanaged ? "var(--warn)" : undefined }}>{a.unmanaged}</b>
          <span>unmanaged</span>
        </div>
        <div>
          <b>{a.external}</b>
          <span>plugin / built-in</span>
        </div>
      </div>

      <div>
        {a.dirs.map((d) => {
          const others = d.readers.filter((r) => r !== a.id);
          return (
            <div className="dir" key={d.path} style={d.exists ? undefined : { opacity: 0.5 }}>
              <div className="dir-path">
                <FolderOpen size={12} style={{ color: "var(--text-3)", flexShrink: 0 }} />
                <span className="ellipsis">{d.path}</span>
                {!d.exists && <span className="faint" style={{ fontFamily: "var(--font)", marginLeft: "auto" }}>not created</span>}
              </div>
              <div className="dir-meta">
                {d.exists && (
                  <>
                    <span>{d.managed} managed</span>
                    {d.unmanaged > 0 && <span style={{ color: "var(--warn)" }}>{d.unmanaged} unmanaged</span>}
                    {d.broken > 0 && <span style={{ color: "var(--error)" }}>{d.broken} broken</span>}
                  </>
                )}
                {others.length > 0 && <span title={others.map(agentName).join(", ")}>shared with {others.slice(0, 3).map(agentName).join(", ")}{others.length > 3 ? ` +${others.length - 3}` : ""}</span>}
              </div>
            </div>
          );
        })}
      </div>

      <div className="agent-foot">
        {a.mcp ? (
          a.mcp.installed ? (
            <span className="done-tick">
              <Check size={13} /> MCP connected
            </span>
          ) : (
            <button className="btn sm" disabled={busy} onClick={connect}>
              <Plug size={13} /> Connect MCP
            </button>
          )
        ) : (
          <span className="faint">MCP setup not automated yet</span>
        )}
        <span className="faint" style={{ marginLeft: "auto" }}>
          {a.budget ? `Listing budget ~${compact(a.budget)} chars` : "No listing budget"}
        </span>
      </div>
    </div>
  );
}

/** library → folders → agents, drawn with the agents actually installed. */
function PlacementDiagram({ agents }: { agents: string[] }) {
  const shown = agents.slice(0, 5);
  const folders = ["~/.agents/skills", "~/.claude/skills", "~/.codex/skills"];
  const W = 360;
  const H = 150;
  const libY = H / 2;
  const fy = (i: number) => 30 + i * 45;
  const ay = (i: number) => (shown.length === 1 ? H / 2 : 16 + (i * (H - 32)) / Math.max(1, shown.length - 1));
  const reads: Record<string, number[]> = { claude: [1], codex: [0, 2], amp: [0, 1], cursor: [0, 2], gemini: [0], opencode: [0, 1], copilot: [0], factory: [0], pi: [0] };
  return (
    <svg width={W} height={H} viewBox={`0 0 ${W} ${H}`} style={{ display: "block", justifySelf: "end" }} aria-label="Library symlinked into agent folders">
      {folders.map((_, i) => (
        <path key={i} d={`M88 ${libY} C 120 ${libY}, 120 ${fy(i)}, 150 ${fy(i)}`} stroke="var(--accent-line)" strokeWidth={1.4} fill="none" strokeDasharray="3 3" />
      ))}
      {shown.map((a, j) =>
        (reads[a] ?? [0]).map((i) => <path key={`${a}-${i}`} d={`M264 ${fy(i)} C 290 ${fy(i)}, 290 ${ay(j)}, 318 ${ay(j)}`} stroke={AGENT_META[a]?.color ?? "#888"} strokeOpacity={0.45} strokeWidth={1.2} fill="none" />),
      )}
      <rect x={6} y={libY - 18} width={82} height={36} rx={9} fill="var(--accent-soft)" stroke="var(--accent-line)" />
      <text x={47} y={libY + 4} textAnchor="middle" fill="var(--accent)" fontSize={11.5} fontWeight={600}>
        Library
      </text>
      {folders.map((f, i) => (
        <g key={f}>
          <rect x={150} y={fy(i) - 12} width={114} height={24} rx={6} fill="var(--bg-3)" stroke="var(--line-2)" />
          <text x={207} y={fy(i) + 4} textAnchor="middle" fill="var(--text-2)" fontSize={9.5} fontFamily="var(--mono)">
            {f}
          </text>
        </g>
      ))}
      {shown.map((a, j) => (
        <g key={a}>
          <circle cx={330} cy={ay(j)} r={11} fill={AGENT_META[a]?.color ?? "#888"} />
          <text x={330} y={ay(j) + 3.5} textAnchor="middle" fontSize={9} fontWeight={700} fill="#0a0a0c">
            {AGENT_META[a]?.short ?? a[0]}
          </text>
        </g>
      ))}
    </svg>
  );
}
