import { useMemo, useState } from "react";
import { Activity, Archive, ChevronDown, ChevronRight, Hand, Moon, Sparkles, Zap } from "lucide-react";
import "./pages.css";
import type { HealthReport, UsageReport } from "../lib/api";
import { useStore } from "../lib/store";
import { AGENT_META, AgentChip, AgentStack, agentName, ago, compact, Empty, Meter, Segmented } from "../components/ui";
import { useApi } from "./pages/useApi";

type Period = "7" | "30" | "90";

export function Insights() {
  const [period, setPeriod] = useState<Period>("30");
  const days = Number(period);
  const usage = useApi<UsageReport>("usage", { days });
  const health = useApi<HealthReport>("health");
  const u = usage.data;

  const stats = useMemo(() => {
    if (!u) return null;
    const byAgent: Record<string, number> = {};
    let total = 0;
    for (const d of u.timeline)
      for (const [a, n] of Object.entries(d.agents)) {
        byAgent[a] = (byAgent[a] ?? 0) + n;
        total += n;
      }
    const top = Object.entries(byAgent).sort((a, b) => b[1] - a[1])[0];
    const manual = u.top.reduce((n, t) => n + Math.min(t.usage.manual, t.usage.recent), 0);
    const totalTop = u.top.reduce((n, t) => n + t.usage.recent, 0);
    return { total, active: u.top.length, top, manual, auto: Math.max(0, totalTop - manual) };
  }, [u]);

  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">Insights</h1>
            <div className="page-sub">Which skills actually earn their place, across every agent and machine.</div>
          </div>
          <div className="page-actions">
            <Segmented<Period>
              value={period}
              onChange={setPeriod}
              options={[
                { value: "7", label: "7 days" },
                { value: "30", label: "30 days" },
                { value: "90", label: "90 days" },
              ]}
            />
          </div>
        </div>

        <div className="pg-grid cols-4">
          <div className="card stat">
            <div className="stat-label">
              <Activity size={13} /> Invocations
            </div>
            <div className="stat-value">{stats ? compact(stats.total) : "–"}</div>
            <div className="stat-sub">in the last {days} days</div>
          </div>
          <div className="card stat">
            <div className="stat-label">
              <Sparkles size={13} /> Active skills
            </div>
            <div className="stat-value">{stats?.active ?? "–"}</div>
            <div className="stat-sub">used at least once</div>
          </div>
          <div className="card stat">
            <div className="stat-label">
              <Zap size={13} /> Busiest agent
            </div>
            <div className="stat-value">
              {stats?.top ? (
                <>
                  <AgentChip id={stats.top[0]} size={24} />
                  <span style={{ fontSize: 19 }}>{agentName(stats.top[0])}</span>
                </>
              ) : (
                "–"
              )}
            </div>
            <div className="stat-sub">{stats?.top ? `${stats.top[1]} invocations` : "no usage yet"}</div>
          </div>
          <div className="card stat">
            <div className="stat-label">
              <Hand size={13} /> Manual vs automatic
            </div>
            <div className="stat-value">
              {stats && stats.manual + stats.auto > 0 ? `${Math.round((stats.auto / (stats.manual + stats.auto)) * 100)}%` : "–"}
              <span className="faint" style={{ fontSize: 12, fontWeight: 500, letterSpacing: 0 }}>
                auto
              </span>
            </div>
            <div className="stat-split" title={stats ? `${stats.auto} automatic · ${stats.manual} typed by you` : ""}>
              <span style={{ width: `${stats && stats.manual + stats.auto ? (stats.auto / (stats.manual + stats.auto)) * 100 : 0}%`, background: "var(--accent)" }} />
              <span style={{ flex: 1, background: "var(--info)", opacity: 0.7 }} />
            </div>
          </div>
        </div>

        <div className="card card-pad" style={{ marginTop: 14 }}>
          <div className="row" style={{ marginBottom: 16 }}>
            <div className="card-title" style={{ margin: 0 }}>
              Daily invocations by agent
            </div>
            <div className="legend" style={{ marginLeft: "auto" }}>
              {u &&
                agentsIn(u).map((a) => (
                  <span key={a}>
                    <i style={{ background: AGENT_META[a]?.color ?? "#888" }} />
                    {agentName(a)}
                  </span>
                ))}
            </div>
          </div>
          {u ? <StackedChart timeline={u.timeline} /> : <div style={{ height: 200 }} />}
        </div>

        <div className="pg-grid cols-2" style={{ marginTop: 14 }}>
          <TopSkills u={u} />
          <GoneDark dark={new Set((health.data?.issues ?? []).filter((i) => i.code === "dark" && i.skill).map((i) => i.skill!))} />
        </div>

        <Budgets health={health.data} />
      </div>
    </div>
  );
}

function agentsIn(u: UsageReport): string[] {
  const set = new Set<string>();
  u.timeline.forEach((d) => Object.keys(d.agents).forEach((a) => set.add(a)));
  return [...set].sort((a, b) => Object.keys(AGENT_META).indexOf(a) - Object.keys(AGENT_META).indexOf(b));
}

function StackedChart({ timeline }: { timeline: UsageReport["timeline"] }) {
  const [hover, setHover] = useState<number | null>(null);
  const W = 960;
  const H = 200;
  const padL = 28;
  const padB = 22;
  const order = Object.keys(AGENT_META);
  const max = Math.max(1, ...timeline.map((d) => Object.values(d.agents).reduce((a, b) => a + b, 0)));
  const nice = niceMax(max);
  const plotW = W - padL;
  const plotH = H - padB;
  const slot = plotW / timeline.length;
  const barW = Math.max(2, Math.min(22, slot * 0.62));
  const ticks = [0, nice / 2, nice];
  const labelEvery = Math.ceil(timeline.length / 8);
  const empty = max <= 1 && timeline.every((d) => Object.keys(d.agents).length === 0);

  return (
    <div className="chart-wrap" onMouseLeave={() => setHover(null)}>
      <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label="Daily skill invocations, stacked by agent" preserveAspectRatio="none" style={{ height: H }}>
        {ticks.map((t) => {
          const y = plotH - (t / nice) * (plotH - 8);
          return (
            <g key={t}>
              <line x1={padL} x2={W} y1={y} y2={y} stroke="rgba(255,255,255,0.05)" strokeDasharray={t === 0 ? undefined : "2 4"} />
              <text x={padL - 8} y={y + 3.5} fill="var(--text-4)" fontSize={10} textAnchor="end" fontFamily="var(--mono)">
                {t}
              </text>
            </g>
          );
        })}
        {timeline.map((d, i) => {
          const x = padL + i * slot + (slot - barW) / 2;
          let y = plotH;
          const entries = Object.entries(d.agents).sort((a, b) => order.indexOf(a[0]) - order.indexOf(b[0]));
          const dim = hover !== null && hover !== i;
          return (
            <g key={d.day} className="chart-col" onMouseEnter={() => setHover(i)}>
              <rect x={padL + i * slot} y={0} width={slot} height={plotH} fill={hover === i ? "rgba(255,255,255,0.03)" : "transparent"} />
              {entries.map(([a, n], j) => {
                const h = (n / nice) * (plotH - 8);
                y -= h;
                const top = j === entries.length - 1;
                return (
                  <rect
                    key={a}
                    className="chart-bar"
                    x={x}
                    y={y + (j > 0 ? 0.5 : 0)}
                    width={barW}
                    height={Math.max(0, h - (j > 0 ? 0.5 : 0))}
                    rx={top ? Math.min(3, barW / 3) : 0}
                    fill={AGENT_META[a]?.color ?? "#888"}
                    opacity={dim ? 0.35 : 0.9}
                  />
                );
              })}
              {i % labelEvery === 0 && (
                <text x={padL + i * slot + slot / 2} y={H - 6} fill="var(--text-4)" fontSize={10} textAnchor="middle">
                  {shortDay(d.day)}
                </text>
              )}
            </g>
          );
        })}
      </svg>
      {empty && (
        <div className="faint" style={{ position: "absolute", inset: 0, display: "grid", placeItems: "center" }}>
          No skill invocations in this period yet.
        </div>
      )}
      {hover !== null && timeline[hover] && (
        <div className="chart-tip" style={{ left: `${((padL + hover * slot + slot / 2) / W) * 100}%`, top: 24 }}>
          <div className="tip-day">{longDay(timeline[hover].day)}</div>
          {Object.keys(timeline[hover].agents).length === 0 && <div className="faint">No invocations</div>}
          {Object.entries(timeline[hover].agents)
            .sort((a, b) => b[1] - a[1])
            .map(([a, n]) => (
              <div className="tip-row" key={a}>
                <i className="dot" style={{ background: AGENT_META[a]?.color ?? "#888", width: 7, height: 7 }} />
                {agentName(a)}
                <b>{n}</b>
              </div>
            ))}
        </div>
      )}
    </div>
  );
}

function niceMax(n: number) {
  if (n <= 4) return 4;
  const mag = 10 ** Math.floor(Math.log10(n));
  for (const m of [1, 2, 2.5, 5, 10]) if (m * mag >= n) return m * mag;
  return 10 * mag;
}

function shortDay(d: string) {
  const dt = new Date(d + "T12:00:00");
  return dt.toLocaleDateString("en-NZ", { day: "numeric", month: "short" });
}
function longDay(d: string) {
  const dt = new Date(d + "T12:00:00");
  return dt.toLocaleDateString("en-NZ", { weekday: "long", day: "numeric", month: "long" });
}

function TopSkills({ u }: { u: UsageReport | null }) {
  const { openSkill } = useStore();
  const top = u?.top.slice(0, 10) ?? [];
  const max = Math.max(1, ...top.map((t) => t.usage.recent));
  return (
    <div className="card card-pad">
      <div className="card-title">Top skills</div>
      {top.length === 0 ? (
        <Empty title="Nothing yet">Usage appears here as your agents call skills.</Empty>
      ) : (
        <div className="rank">
          {top.map((t, i) => (
            <div className="rank-row" key={t.skill} onClick={() => openSkill(t.skill)}>
              <span className="rank-idx">{i + 1}</span>
              <div style={{ minWidth: 0 }}>
                <div className="rank-name">
                  <span className="ellipsis" style={{ fontWeight: 500 }}>
                    {t.skill}
                  </span>
                  <AgentStack agents={Object.keys(t.usage.by_agent)} max={4} />
                </div>
                <div className="rank-bar">
                  <span style={{ width: `${(t.usage.recent / max) * 100}%` }} />
                </div>
              </div>
              <span className="rank-count">{t.usage.recent}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function GoneDark({ dark: flagged }: { dark: Set<string> }) {
  const { skills, run, openSkill } = useStore();
  const dark = skills
    .filter((s) => s.kind === "managed" && !s.archived && s.usage.recent === 0)
    .sort((a, b) => Number(flagged.has(b.name)) - Number(flagged.has(a.name)) || (a.usage.last_ts ?? 0) - (b.usage.last_ts ?? 0));
  return (
    <div className="card card-pad">
      <div className="card-title">
        <Moon size={13} /> Gone dark
        <span className="faint" style={{ marginLeft: "auto", fontWeight: 400 }}>
          Unused in 30 days
        </span>
      </div>
      {dark.length === 0 ? (
        <Empty title="Every skill is pulling its weight">Unused skills show up here so you can archive them.</Empty>
      ) : (
        <div>
          {dark.slice(0, 10).map((s) => (
            <div className="list-row" key={s.name}>
              <div className="grow" style={{ minWidth: 0 }}>
                <button className="link ellipsis" onClick={() => openSkill(s.name)}>
                  {s.name}
                </button>
                <div className="faint" style={{ fontSize: 11.5 }}>
                  {flagged.has(s.name) ? (
                    <span style={{ color: "var(--warn)" }}>Gone dark</span>
                  ) : (
                    <span>New, give it time</span>
                  )}{" "}
                  · {s.usage.last_ts ? `last used ${ago(s.usage.last_ts)}` : "never used"} · ~{s.tokens} tokens
                </div>
              </div>
              <button className="btn sm ghost" onClick={() => run("set_archived", { name: s.name, archived: true }, `Archived ${s.name}`)}>
                <Archive size={13} /> Archive
              </button>
            </div>
          ))}
          {dark.length > 10 && <div className="faint" style={{ paddingTop: 8, fontSize: 12 }}>and {dark.length - 10} more</div>}
        </div>
      )}
    </div>
  );
}

function Budgets({ health }: { health: HealthReport | null }) {
  const [open, setOpen] = useState<Record<string, boolean>>({});
  const { openSkill } = useStore();
  if (!health || health.budgets.length === 0) return null;
  return (
    <div className="pg-section">
      <div className="pg-section-head">
        <h2>Context budget</h2>
        <p>Every skill's description is loaded into every session. Past the budget, agents silently cut or hide them, least-used first.</p>
      </div>
      <div className="card card-pad">
        {health.budgets.map((b) => {
          const over = b.used > b.budget;
          return (
            <div className="budget-row" key={b.agent}>
              <div className="row" style={{ marginBottom: 9 }}>
                <AgentChip id={b.agent} />
                <span style={{ fontWeight: 500 }}>{b.name}</span>
                <span className="faint">{b.skills} skills</span>
                <span className="num" style={{ marginLeft: "auto", color: over ? "var(--warn)" : "var(--text-2)" }}>
                  {compact(b.used)} / {compact(b.budget)} chars
                </span>
              </div>
              <Meter value={b.used} max={b.budget} over={over} />
              <div className="row" style={{ marginTop: 8, fontSize: 11.5 }}>
                <span className="faint">{b.note}</span>
                {b.at_risk.length > 0 && (
                  <button className="btn ghost sm" style={{ marginLeft: "auto", color: "var(--warn)" }} onClick={() => setOpen((o) => ({ ...o, [b.agent]: !o[b.agent] }))}>
                    {open[b.agent] ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
                    {b.at_risk.length} at risk
                  </button>
                )}
              </div>
              {open[b.agent] && (
                <div className="budget-risk fade-in">
                  {b.at_risk.map((n) => (
                    <button key={n} className="chip" onClick={() => openSkill(n)}>
                      {n}
                    </button>
                  ))}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
