import { useEffect, useRef, useState, type ReactNode } from "react";
import { X } from "lucide-react";

// ----------------------------------------------------------------- agents

export const AGENT_META: Record<string, { name: string; color: string; short: string }> = {
  claude: { name: "Claude Code", color: "#d97757", short: "C" },
  codex: { name: "Codex", color: "#e8e8ea", short: "Cx" },
  amp: { name: "Amp", color: "#f2545b", short: "A" },
  cursor: { name: "Cursor", color: "#8f9bb3", short: "Cu" },
  gemini: { name: "Gemini CLI", color: "#5b8def", short: "G" },
  opencode: { name: "OpenCode", color: "#a3a3ff", short: "O" },
  copilot: { name: "Copilot CLI", color: "#a371f7", short: "Co" },
  factory: { name: "Factory Droid", color: "#f5a524", short: "F" },
  pi: { name: "Pi", color: "#e879f9", short: "π" },
};
export const AGENT_IDS = Object.keys(AGENT_META);

export function agentName(id: string) {
  return AGENT_META[id]?.name ?? id;
}

export function AgentChip({ id, off, size = 18 }: { id: string; off?: boolean; size?: number }) {
  const m = AGENT_META[id] ?? { name: id, color: "#888", short: id.slice(0, 1).toUpperCase() };
  return (
    <span
      className={`agent-chip${off ? " off" : ""}`}
      title={m.name}
      style={{ background: m.color, width: size, height: size, fontSize: size * 0.5 }}
    >
      {m.short}
    </span>
  );
}

/** Agent ids as overlapping chips; ["*"] renders as "All agents". */
export function AgentStack({ agents, max = 5 }: { agents: string[]; max?: number }) {
  if (agents.includes("*")) return <span className="agent-all">All</span>;
  const shown = agents.slice(0, max);
  return (
    <span className="agent-stack" title={agents.map(agentName).join(", ")}>
      {shown.map((a) => (
        <AgentChip key={a} id={a} />
      ))}
      {agents.length > max && <span className="faint" style={{ marginLeft: 4, fontSize: 11 }}>+{agents.length - max}</span>}
    </span>
  );
}

// ----------------------------------------------------------------- small bits

export function Kbd({ children }: { children: ReactNode }) {
  return <span className="kbd">{children}</span>;
}

export function Badge({ children, tone, title }: { children: ReactNode; tone?: "accent" | "error" | "warn" | "info" | "outline"; title?: string }) {
  return (
    <span className={`badge${tone ? ` ${tone}` : ""}`} title={title}>
      {children}
    </span>
  );
}

export function Sev({ s }: { s: "error" | "warn" | "info" }) {
  return <span className={`sev ${s}`} />;
}

export function Segmented<T extends string>({ value, options, onChange }: { value: T; options: { value: T; label: ReactNode }[]; onChange: (v: T) => void }) {
  return (
    <div className="segmented">
      {options.map((o) => (
        <button key={o.value} className={o.value === value ? "on" : ""} onClick={() => onChange(o.value)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function Meter({ value, max, over }: { value: number; max: number; over?: boolean }) {
  const pct = Math.min(100, max > 0 ? (value / max) * 100 : 0);
  return (
    <div className={`meter${over ? " over" : ""}`}>
      <span style={{ width: `${pct}%` }} />
    </div>
  );
}

export function Sparkline({ data, width = 64, height = 18, color = "var(--accent)" }: { data: number[]; width?: number; height?: number; color?: string }) {
  const id = useRef(`sg${Math.random().toString(36).slice(2, 8)}`).current;
  if (!data.length) return null;
  const max = Math.max(1, ...data);
  const step = width / Math.max(1, data.length - 1);
  const pts = data.map((v, i) => [i * step, height - 1.5 - (v / max) * (height - 3)] as const);
  const line = pts.map(([x, y], i) => `${i ? "L" : "M"}${x.toFixed(1)},${y.toFixed(1)}`).join(" ");
  const area = `${line} L${width},${height} L0,${height} Z`;
  const empty = data.every((v) => v === 0);
  return (
    <svg width={width} height={height} style={{ display: "block", overflow: "visible" }}>
      <defs>
        <linearGradient id={id} x1="0" x2="0" y1="0" y2="1">
          <stop offset="0%" stopColor={color} stopOpacity={0.22} />
          <stop offset="100%" stopColor={color} stopOpacity={0} />
        </linearGradient>
      </defs>
      {!empty && <path d={area} fill={`url(#${id})`} />}
      <path d={line} fill="none" stroke={empty ? "var(--text-4)" : color} strokeWidth={1.3} strokeLinejoin="round" strokeLinecap="round" opacity={empty ? 0.5 : 1} />
    </svg>
  );
}

export function Empty({ icon, title, children, action }: { icon?: ReactNode; title: string; children?: ReactNode; action?: ReactNode }) {
  return (
    <div className="empty fade-in">
      {icon && <div className="empty-icon">{icon}</div>}
      <h3>{title}</h3>
      {children && <p>{children}</p>}
      {action && <div style={{ marginTop: 6 }}>{action}</div>}
    </div>
  );
}

export function Modal({ title, subtitle, children, footer, onClose, width }: { title: string; subtitle?: ReactNode; children: ReactNode; footer?: ReactNode; onClose: () => void; width?: number }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  return (
    <div className="overlay" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal" style={width ? { width: `min(${width}px, calc(100vw - 48px))` } : undefined}>
        <div className="modal-head row" style={{ alignItems: "flex-start" }}>
          <div className="grow">
            <h2>{title}</h2>
            {subtitle && <p>{subtitle}</p>}
          </div>
          <button className="btn ghost icon sm" onClick={onClose} aria-label="Close">
            <X size={15} />
          </button>
        </div>
        <div className="modal-body">{children}</div>
        {footer && <div className="modal-foot">{footer}</div>}
      </div>
    </div>
  );
}

/** Click-to-open dropdown anchored to its trigger. */
export function Dropdown({ trigger, children, align = "right" }: { trigger: (open: () => void) => ReactNode; children: (close: () => void) => ReactNode; align?: "left" | "right" }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => !ref.current?.contains(e.target as Node) && setOpen(false);
    window.addEventListener("mousedown", onDown);
    return () => window.removeEventListener("mousedown", onDown);
  }, [open]);
  return (
    <div ref={ref} style={{ position: "relative", display: "inline-flex" }}>
      {trigger(() => setOpen((o) => !o))}
      {open && (
        <div className="menu" style={{ top: "calc(100% + 6px)", [align]: 0 }}>
          {children(() => setOpen(false))}
        </div>
      )}
    </div>
  );
}

// ----------------------------------------------------------------- formatting

export function ago(ts: number | null | undefined): string {
  if (!ts) return "never";
  const s = Math.max(0, (Date.now() - ts) / 1000);
  if (s < 45) return "just now";
  if (s < 3600) return `${Math.round(s / 60)}m ago`;
  if (s < 86400) return `${Math.round(s / 3600)}h ago`;
  if (s < 86400 * 30) return `${Math.round(s / 86400)}d ago`;
  return new Date(ts).toLocaleDateString("en-NZ", { day: "numeric", month: "short", year: "numeric" });
}

export function fmtDate(ts: number): string {
  return new Date(ts).toLocaleString("en-NZ", { day: "numeric", month: "short", hour: "numeric", minute: "2-digit" });
}

export function compact(n: number): string {
  if (n >= 10000) return `${(n / 1000).toFixed(0)}k`;
  if (n >= 1000) return `${(n / 1000).toFixed(1)}k`;
  return String(n);
}

/** Human label for a version author. */
export function authorLabel(a: string): { label: string; tone: "accent" | "info" | "warn" | undefined } {
  if (a === "you") return { label: "You", tone: "accent" };
  if (a.startsWith("agent:")) return { label: a.slice(6), tone: "info" };
  if (a === "agent") return { label: "Agent", tone: "info" };
  if (a === "external") return { label: "Outside edit", tone: "warn" };
  if (a === "sync") return { label: "Sync merge", tone: undefined };
  if (a === "adopt") return { label: "Adopted", tone: undefined };
  if (a === "import") return { label: "Imported", tone: undefined };
  if (a === "upstream") return { label: "Upstream", tone: undefined };
  if (a === "upskill") return { label: "UpSkill", tone: undefined };
  return { label: a, tone: undefined };
}
