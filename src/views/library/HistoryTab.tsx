import { useEffect, useState } from "react";
import { RotateCcw } from "lucide-react";
import { call, type FileDiff, type HistoryEntry, type SkillDetail } from "../../lib/api";
import { useStore } from "../../lib/store";
import { ago, authorLabel, Badge, Empty, fmtDate, Segmented, Sparkline } from "../../components/ui";

export function HistoryTab({ detail }: { detail: SkillDetail }) {
  const s = useStore();
  const name = detail.name;
  const [hist, setHist] = useState<HistoryEntry[] | null>(null);
  const [sel, setSel] = useState<string | null>(null);
  const [mode, setMode] = useState<"change" | "current">("change");
  const [diff, setDiff] = useState<FileDiff[] | null>(null);

  useEffect(() => {
    call<HistoryEntry[]>("history", { name })
      .then((h) => {
        setHist(h);
        setSel((cur) => (cur && h.some((x) => x.id === cur) ? cur : (h[0]?.id ?? null)));
      })
      .catch((e) => s.toast(String(e), "error"));
  }, [name, detail.meta.head, s]);

  useEffect(() => {
    if (!sel) return;
    setDiff(null);
    call<FileDiff[]>("diff", mode === "change" ? { name, to: sel } : { name, from: sel })
      .then(setDiff)
      .catch(() => setDiff([]));
  }, [sel, mode, name, detail.meta.head]);

  if (!hist) return <div className="det-loading" />;
  if (hist.length === 0) return <Empty title="No history yet">Versions appear here every time the skill changes.</Empty>;

  const entry = hist.find((h) => h.id === sel) ?? hist[0];
  const trend = [...hist].reverse().map((h) => h.tokens);
  const first = trend[0] ?? 0;
  const last = trend[trend.length - 1] ?? 0;
  const growth = first > 0 ? Math.round(((last - first) / first) * 100) : 0;

  return (
    <div className="hist">
      <div className="hist-list">
        <div className="hist-trend" title="SKILL.md size across versions">
          <Sparkline data={trend.length > 1 ? trend : [last, last]} width={120} height={22} color={growth > 40 ? "var(--warn)" : "var(--accent)"} />
          <div>
            <div className="num" style={{ color: "var(--text-2)" }}>
              ~{last} tokens
            </div>
            <div className={growth > 40 ? "warn-text" : ""}>{hist.length > 1 ? `${growth >= 0 ? "+" : ""}${growth}% since first version` : "First version"}</div>
          </div>
        </div>
        {hist.map((h) => {
          const a = authorLabel(h.author);
          return (
            <button key={h.id} className={`hist-item${h.id === entry.id ? " on" : ""}${h.is_head ? " head" : ""}`} onClick={() => setSel(h.id)}>
              <span className="hist-dot" />
              <div className="row" style={{ gap: 6 }}>
                <Badge tone={a.tone}>{a.label}</Badge>
                {h.is_head && <span className="hist-current">Current</span>}
                <span className="grow" />
                <span className="faint hist-ago" title={fmtDate(h.ts)}>
                  {ago(h.ts)}
                </span>
              </div>
              <div className="hist-msg">{h.message}</div>
              <div className="hist-meta">
                {h.machine && <span>{h.machine}</span>}
                <span className="add">+{h.added}</span>
                <span className="del">−{h.removed}</span>
                <span>~{h.tokens} tok</span>
              </div>
            </button>
          );
        })}
      </div>
      <div className="hist-diff">
        <div className="hist-diff-head">
          <Segmented
            value={mode}
            onChange={setMode}
            options={[
              { value: "change", label: "What changed" },
              { value: "current", label: "Compare with current" },
            ]}
          />
          <span className="grow" />
          <span className="faint mono" style={{ fontSize: 11 }}>
            {entry.id.slice(0, 7)}
          </span>
          {!entry.is_head && (
            <button className="btn sm" onClick={() => s.run("restore_version", { name, version: entry.id }, "Restored. The newer version is still in history")}>
              <RotateCcw size={13} /> Restore this version
            </button>
          )}
        </div>
        {diff === null ? (
          <div className="det-loading" />
        ) : diff.length === 0 ? (
          <Empty title={mode === "current" ? "Identical to the current version" : "No file changes"}>
            {mode === "current" ? "Nothing to restore here." : "This version only changed metadata."}
          </Empty>
        ) : (
          diff.map((f) => <DiffFile key={f.path} f={f} />)
        )}
      </div>
    </div>
  );
}

export function DiffFile({ f }: { f: FileDiff }) {
  const lines = f.diff.split("\n").filter((l) => !l.startsWith("---") && !l.startsWith("+++"));
  return (
    <div className="diff-file">
      <div className="diff-file-head">
        <span className="ellipsis">{f.path}</span>
        {f.status !== "modified" && <Badge tone={f.status === "added" ? "accent" : "error"}>{f.status}</Badge>}
        <span className="grow" />
        <span className="add">+{f.added}</span>
        <span className="del">−{f.removed}</span>
      </div>
      <div className="diff-lines">
        {lines.map((l, i) => {
          const cls = l.startsWith("@@") ? "hunk" : l.startsWith("+") ? "add" : l.startsWith("-") ? "del" : "ctx";
          return (
            <div key={i} className={`diff-line ${cls}`}>
              {l || " "}
            </div>
          );
        })}
      </div>
    </div>
  );
}
