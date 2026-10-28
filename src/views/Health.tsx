import { useMemo, useState } from "react";
import { Archive, Check, Download, GitCompare, Link2Off, Sparkles, Wand2 } from "lucide-react";
import "./pages.css";
import type { HealthIssue, HealthReport, Severity } from "../lib/api";
import { useStore } from "../lib/store";
import { Empty, Sev } from "../components/ui";
import { useApi } from "./pages/useApi";
import { Ticks } from "./pages/Ticks";

const GROUPS: { key: string; label: string; match: (c: string) => boolean }[] = [
  { key: "spec", label: "Spec", match: (c) => /^(no-|bad-yaml|name-|empty-body|dup-)/.test(c) },
  { key: "trigger", label: "Triggers", match: (c) => c.startsWith("description") || c === "overlap" },
  { key: "size", label: "Size & bloat", match: (c) => c.startsWith("body") || c === "bloat" || c === "budget-over" },
  { key: "security", label: "Security", match: (c) => c.startsWith("sec-") },
  { key: "folders", label: "Folders", match: (c) => ["broken-link", "unmanaged", "divergent-copies"].includes(c) },
  { key: "rot", label: "Unused", match: (c) => c === "dark" },
  { key: "notes", label: "Notes", match: (c) => c === "note" },
];

function groupOf(code: string) {
  return GROUPS.find((g) => g.match(code))?.key ?? "other";
}

const SECTIONS: { sev: Severity; title: string; sub: string }[] = [
  { sev: "error", title: "Problems", sub: "Agents will skip or misread these skills, or they could do harm." },
  { sev: "warn", title: "Warnings", sub: "Working, but costing you reliability or context." },
  { sev: "info", title: "Suggestions", sub: "Small things that make skills sharper." },
];

export function Health() {
  const { data, loading } = useApi<HealthReport>("health");
  const { run } = useStore();
  const [sev, setSev] = useState<Severity | "all">("all");
  const [group, setGroup] = useState<string>("all");

  const counts = useMemo(() => {
    const c = { error: 0, warn: 0, info: 0 } as Record<Severity, number>;
    data?.issues.forEach((i) => c[i.severity]++);
    return c;
  }, [data]);

  const groupCounts = useMemo(() => {
    const c: Record<string, number> = {};
    data?.issues.forEach((i) => (c[groupOf(i.code)] = (c[groupOf(i.code)] ?? 0) + 1));
    return c;
  }, [data]);

  const filtered = (data?.issues ?? []).filter((i) => (sev === "all" || i.severity === sev) && (group === "all" || groupOf(i.code) === group));
  const safeFixes = (data?.issues ?? []).filter((i) => i.action === "cleanup_broken" || i.action === "adopt").length;

  const fixAll = async () => {
    if (!confirm("Remove broken links and adopt every unmanaged skill? Originals are backed up to ~/.upskill/backups and every differing copy is kept in history.")) return;
    await run("cleanup_broken");
    await run("adopt_all", {}, "Fixed all safe issues");
  };

  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">Health</h1>
            <div className="page-sub">Spec checks, trigger collisions, security, context budget and rot, all in one place.</div>
          </div>
          <div className="page-actions">
            <button className="btn primary" disabled={!safeFixes} onClick={fixAll}>
              <Wand2 size={14} /> Fix all safe issues
            </button>
          </div>
        </div>

        <div className="card score-card">
          <ScoreRing score={data?.score ?? 0} loading={loading && !data} />
          <div className="grow">
            <div style={{ fontSize: 15, fontWeight: 600, letterSpacing: "-0.01em" }}>{verdict(data?.score, counts.error)}</div>
            <div className="faint" style={{ marginTop: 3 }}>
              Errors cost 8 points each, warnings 3. Suggestions are free.
            </div>
            <div className="sev-counts">
              {(["error", "warn", "info"] as Severity[]).map((s) => (
                <div key={s}>
                  <b style={{ color: counts[s] && s === "error" ? "var(--error)" : undefined }}>{counts[s]}</b>
                  <span>
                    <Sev s={s} /> {s === "error" ? "Problems" : s === "warn" ? "Warnings" : "Suggestions"}
                  </span>
                </div>
              ))}
            </div>
          </div>
        </div>

        <div className="row" style={{ marginTop: 18, flexWrap: "wrap", gap: 12 }}>
          <div className="chips">
            {(["all", "error", "warn", "info"] as const).map((s) => (
              <button key={s} className={`chip${sev === s ? " on" : ""}`} onClick={() => setSev(s)}>
                {s !== "all" && <Sev s={s} />}
                {s === "all" ? "All" : s === "error" ? "Problems" : s === "warn" ? "Warnings" : "Suggestions"}
                <span className="n">{s === "all" ? data?.issues.length ?? 0 : counts[s]}</span>
              </button>
            ))}
          </div>
          <div style={{ width: 1, height: 18, background: "var(--line-2)" }} />
          <div className="chips">
            <button className={`chip${group === "all" ? " on" : ""}`} onClick={() => setGroup("all")}>
              Everything
            </button>
            {GROUPS.filter((g) => groupCounts[g.key]).map((g) => (
              <button key={g.key} className={`chip${group === g.key ? " on" : ""}`} onClick={() => setGroup(g.key)}>
                {g.label} <span className="n">{groupCounts[g.key]}</span>
              </button>
            ))}
          </div>
        </div>

        {data && filtered.length === 0 && (
          <div className="card" style={{ marginTop: 16 }}>
            <Empty icon={<Sparkles size={20} />} title="All clear">
              Nothing to fix here. Your skills are lean, valid and placed where they belong.
            </Empty>
          </div>
        )}

        {SECTIONS.map(({ sev: s, title, sub }) => {
          const items = filtered.filter((i) => i.severity === s);
          if (!items.length) return null;
          return (
            <div className="pg-section" key={s}>
              <div className="pg-section-head">
                <h2>{title}</h2>
                <p>{sub}</p>
                <span className="right faint num">{items.length}</span>
              </div>
              <div className="card">
                {items.slice(0, 80).map((i, n) => (
                  <IssueRow key={`${i.code}-${i.skill}-${n}`} issue={i} />
                ))}
                {items.length > 80 && <div className="faint" style={{ padding: "10px 18px" }}>and {items.length - 80} more</div>}
              </div>
            </div>
          );
        })}

        {data && data.overlaps.length > 0 && (sev === "all" || sev === "warn") && (group === "all" || group === "trigger") && (
          <div className="pg-section">
            <div className="pg-section-head">
              <h2>Overlapping triggers</h2>
              <p>Pairs whose descriptions compete for the same requests. Agents pick between them at random.</p>
            </div>
            <div className="card">
              {data.overlaps.slice(0, 20).map(([a, b, sim]) => (
                <Overlap key={`${a}-${b}`} a={a} b={b} sim={sim} />
              ))}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function verdict(score: number | undefined, errors: number) {
  if (score === undefined) return "Checking your skills…";
  if (errors > 0) return `${errors} problem${errors === 1 ? " needs" : "s need"} attention`;
  if (score >= 90) return "Your library is in great shape";
  if (score >= 70) return "Healthy, with a few rough edges";
  return "Your skills need some tidying";
}

function ScoreRing({ score, loading }: { score: number; loading: boolean }) {
  const r = 48;
  const c = 2 * Math.PI * r;
  const color = score >= 80 ? "var(--accent)" : score >= 55 ? "var(--warn)" : "var(--error)";
  return (
    <div className="score-ring">
      <svg width={112} height={112} viewBox="0 0 112 112" style={{ overflow: "visible" }}>
        <circle cx={56} cy={56} r={r} fill="none" stroke="var(--bg-3)" strokeWidth={8} />
        <circle
          cx={56}
          cy={56}
          r={r}
          fill="none"
          stroke={color}
          strokeWidth={8}
          strokeLinecap="round"
          strokeDasharray={c}
          strokeDashoffset={loading ? c : c * (1 - score / 100)}
          style={{ transition: "stroke-dashoffset 0.8s cubic-bezier(0.2,0.8,0.2,1)", filter: `drop-shadow(0 0 5px ${color})` }}
          opacity={0.95}
        />
      </svg>
      <div className="score-num">
        <div>
          <b>{loading ? "–" : score}</b>
          <span>score</span>
        </div>
      </div>
    </div>
  );
}

function looksLikePath(s: string) {
  return /^[~./\w-]+(\/[\w.-]+)+(:\d+)?$/.test(s) || /^[\w./-]+\.(md|sh|py|ts|js):?\d*$/.test(s);
}

function IssueRow({ issue: i }: { issue: HealthIssue }) {
  const { run, openSkill } = useStore();
  const [busy, setBusy] = useState(false);
  const act = async (fn: () => Promise<unknown>) => {
    setBusy(true);
    await fn();
    setBusy(false);
  };
  const fix = (() => {
    switch (i.action) {
      case "adopt":
        return i.skill && { label: "Adopt", icon: <Download size={13} />, go: () => act(() => run("adopt", { name: i.skill }, `Adopted ${i.skill}`)) };
      case "cleanup_broken":
        return { label: "Remove", icon: <Link2Off size={13} />, go: () => act(() => run("cleanup_broken", {}, "Removed broken links")) };
      case "set_archived":
        return i.skill && { label: "Archive", icon: <Archive size={13} />, go: () => act(() => run("set_archived", { name: i.skill, archived: true }, `Archived ${i.skill}`)) };
      case "diff":
        return i.skill && { label: "Review diff", icon: <GitCompare size={13} />, go: () => openSkill(i.skill!) };
      default:
        return null;
    }
  })();
  const managedLink = i.skill && i.action !== "adopt" && i.code !== "broken-link";
  return (
    <div className="issue">
      <Sev s={i.severity} />
      <div style={{ minWidth: 0 }}>
        <div className="issue-title">
          <Ticks text={i.title} />
        </div>
        {i.detail && (
          <div className={`issue-detail${looksLikePath(i.detail) ? " mono" : ""}`} style={looksLikePath(i.detail) ? { fontSize: 11.5 } : undefined}>
            <Ticks text={i.detail} />
          </div>
        )}
        <div className="issue-meta">
          {managedLink && (
            <button className="link" style={{ fontSize: 11.5 }} onClick={() => openSkill(i.skill!)}>
              {i.skill}
            </button>
          )}
          <span className="faint mono" style={{ fontSize: 10.5 }}>
            {i.code}
          </span>
        </div>
      </div>
      <div className="issue-actions">
        {fix && (
          <button className="btn sm" disabled={busy} onClick={fix.go}>
            {busy ? <Check size={13} /> : fix.icon} {fix.label}
          </button>
        )}
      </div>
    </div>
  );
}

function Overlap({ a, b, sim }: { a: string; b: string; sim: number }) {
  const { openSkill, skills } = useStore();
  const managed = new Set(skills.filter((s) => s.kind === "managed").map((s) => s.name));
  const name = (n: string) =>
    managed.has(n) ? (
      <button className="link" onClick={() => openSkill(n)}>
        {n}
      </button>
    ) : (
      <span className="muted">{n}</span>
    );
  const pct = Math.round(sim * 100);
  return (
    <div className="sim">
      <div className="row ellipsis" style={{ gap: 8 }}>
        {name(a)}
        <span className="faint">and</span>
        {name(b)}
      </div>
      <div className="meter" style={{ height: 4 }}>
        <span style={{ width: `${pct}%`, background: pct >= 70 ? "var(--error)" : "var(--warn)" }} />
      </div>
      <span className="num faint" style={{ textAlign: "right" }}>
        {pct}%
      </span>
    </div>
  );
}
