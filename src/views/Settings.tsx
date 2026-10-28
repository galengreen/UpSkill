import { useEffect, useState } from "react";
import { Check, ChevronsUp, Copy, Plug, Sparkles } from "lucide-react";
import "./pages.css";
import type { Config, EventEntry } from "../lib/api";
import { useStore } from "../lib/store";
import { AgentChip, ago, agentName, Segmented } from "../components/ui";
import { useApi } from "./pages/useApi";

interface McpStatus {
  bin: string | null;
  targets: { agent: string; config: string; installed: boolean }[];
}

export function Settings() {
  const { run, status, agents, skills, toast } = useStore();
  const cfg = useApi<Config>("settings_get");
  const mcp = useApi<McpStatus>("mcp_status");
  const events = useApi<EventEntry[]>("events", { limit: 30 });
  const [name, setName] = useState("");
  const [budgets, setBudgets] = useState<Record<string, string>>({});

  useEffect(() => {
    if (!cfg.data) return;
    setName(cfg.data.machine_name);
    setBudgets(Object.fromEntries(Object.entries(cfg.data.budgets).map(([k, v]) => [k, String(v)])));
  }, [cfg.data]);

  const set = (params: Record<string, unknown>, msg = "Saved") => run("settings_set", params, msg);
  const c = cfg.data;
  const bin = mcp.data?.bin ?? "upskill";
  const hasCompanion = skills.some((s) => s.name === "upskill" && s.kind === "managed");
  const budgetAgents = agents.filter((a) => a.installed && (a.budget !== null || budgets[a.id]));

  const copy = (text: string) => {
    navigator.clipboard?.writeText(text);
    toast("Copied");
  };

  return (
    <div className="page">
      <div className="page-inner" style={{ maxWidth: 860 }}>
        <div className="page-head">
          <div>
            <h1 className="page-title">Settings</h1>
            <div className="page-sub">Few knobs, sensible defaults.</div>
          </div>
        </div>

        <Section title="This machine">
          <Row label="Machine name" hint="How this machine shows up on your other machines and in history.">
            <div className="row">
              <input className="input" style={{ maxWidth: 280 }} value={name} onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && set({ machine_name: name })} />
              <button className="btn" disabled={!c || name.trim() === c.machine_name || !name.trim()} onClick={() => set({ machine_name: name }, "Renamed this machine")}>
                Save
              </button>
            </div>
          </Row>
          <Row label="Auto-sync" hint="How often UpSkill syncs in the background while it's running.">
            <Segmented<string>
              value={String(c?.auto_sync_minutes ?? 10)}
              onChange={(v) => set({ auto_sync_minutes: Number(v) })}
              options={[
                { value: "0", label: "Off" },
                { value: "5", label: "5 min" },
                { value: "10", label: "10 min" },
                { value: "30", label: "30 min" },
                { value: "60", label: "Hourly" },
              ]}
            />
          </Row>
          <Row label="Gone dark after" hint="Skills unused for this long are flagged so you can archive them.">
            <Segmented<string>
              value={String(c?.dark_days ?? 30)}
              onChange={(v) => set({ dark_days: Number(v) })}
              options={[14, 30, 60, 90].map((d) => ({ value: String(d), label: `${d} days` }))}
            />
          </Row>
        </Section>

        <Section title="Context budgets" sub="Roughly how many characters of skill descriptions each agent keeps in context. Raise these if you run a 1M-token model.">
          {budgetAgents.map((a) => (
            <Row key={a.id} label={<span className="row"><AgentChip id={a.id} />{a.name}</span>} hint={`Default ${(agentDefault(a.id) ?? a.budget ?? 0).toLocaleString("en-NZ")} chars`}>
              <div className="row">
                <input
                  className="input budget-input num"
                  inputMode="numeric"
                  placeholder="Default"
                  value={budgets[a.id] ?? ""}
                  onChange={(e) => setBudgets((b) => ({ ...b, [a.id]: e.target.value.replace(/\D/g, "") }))}
                  onBlur={() => {
                    const v = budgets[a.id];
                    const prev = c?.budgets[a.id];
                    if ((v ? Number(v) : undefined) !== prev) set({ budgets: { [a.id]: v ? Number(v) : null } }, `Budget for ${a.name} updated`);
                  }}
                />
                <span className="faint">chars</span>
              </div>
            </Row>
          ))}
        </Section>

        <Section title="Agent access (MCP)" sub="Let agents create, edit, organise and sync skills through UpSkill. Every change they make is versioned and attributed to them.">
          {(mcp.data?.targets ?? []).map((t) => {
            const installed = agents.find((a) => a.id === t.agent)?.installed;
            return (
              <Row key={t.agent} label={<span className="row"><AgentChip id={t.agent} off={!installed} />{agentName(t.agent)}</span>} hint={t.config}>
                {t.installed ? (
                  <span className="done-tick">
                    <Check size={13} /> Connected
                  </span>
                ) : (
                  <button className="btn sm" disabled={!installed || !mcp.data?.bin} onClick={async () => {
                    const out = await run<{ message: string }>("mcp_install", { agent: t.agent });
                    if (out?.message) toast(out.message);
                  }}>
                    <Plug size={13} /> {installed ? "Connect" : "Not installed"}
                  </button>
                )}
              </Row>
            );
          })}
          <div style={{ padding: "14px 20px", borderTop: "1px solid var(--line)" }} className="col">
            <span className="label">Server command</span>
            <CodeBox text={`${bin} mcp`} onCopy={copy} />
            <span className="label" style={{ marginTop: 12 }}>
              Claude Code, by hand
            </span>
            <CodeBox text={`claude mcp add --scope user upskill -- ${bin} mcp`} onCopy={copy} />
          </div>
        </Section>

        <Section title="Companion skill">
          <Row
            label="The upskill skill"
            hint="A tiny skill that tells every agent to manage skills through UpSkill: lint before saving, keep descriptions as triggers, fix misfires with the smallest edit, and leave notes."
          >
            {hasCompanion ? (
              <span className="done-tick">
                <Check size={13} /> In your library
              </span>
            ) : (
              <button className="btn sm" onClick={() => run("seed_companion", {}, "Added the upskill skill")}>
                <Sparkles size={13} /> Add to library
              </button>
            )}
          </Row>
        </Section>

        <Section title="Activity">
          {(events.data ?? []).length === 0 && <div className="faint" style={{ padding: "16px 20px" }}>Nothing yet.</div>}
          {(events.data ?? []).map((e, i) => (
            <div className="event" key={i}>
              <span className="faint">{ago(e.ts)}</span>
              <span className="faint mono ellipsis" style={{ fontSize: 11 }}>
                {e.kind}
              </span>
              <span className="ellipsis" title={e.message}>
                {e.skill && <b style={{ fontWeight: 500 }}>{e.skill} · </b>}
                {e.message}
              </span>
            </div>
          ))}
        </Section>

        <Section title="Data">
          <Row label="Library" hint="One folder per skill. Agents' skill folders are symlinks into here.">
            <CodeBox text={status?.library ?? "~/.upskill/library"} onCopy={copy} />
          </Row>
          <Row label="Backups" hint="Originals of everything UpSkill adopted or removed.">
            <CodeBox text={(status?.library ?? "~/.upskill/library").replace(/library$/, "backups")} onCopy={copy} />
          </Row>
        </Section>

        <Section title="About">
          <div className="row" style={{ padding: "16px 20px", gap: 14 }}>
            <span className="brand-mark" style={{ width: 34, height: 34, borderRadius: 9 }}>
              <ChevronsUp size={20} strokeWidth={2.6} />
            </span>
            <div>
              <div style={{ fontWeight: 600 }}>UpSkill 0.1</div>
              <div className="faint" style={{ fontSize: 12 }}>
                One library for every agent's skills, on every machine. Machine ID <span className="mono selectable">{c?.machine_id}</span>
              </div>
            </div>
          </div>
        </Section>
      </div>
    </div>
  );
}

function agentDefault(id: string): number | undefined {
  return { claude: 8000, codex: 8000 }[id];
}

function Section({ title, sub, children }: { title: string; sub?: string; children: React.ReactNode }) {
  return (
    <div className="pg-section" style={{ marginTop: 22 }}>
      <div className="pg-section-head">
        <h2>{title}</h2>
      </div>
      {sub && (
        <p className="faint" style={{ margin: "-6px 0 12px", fontSize: 12, maxWidth: 640 }}>
          {sub}
        </p>
      )}
      <div className="card">{children}</div>
    </div>
  );
}

function Row({ label, hint, children }: { label: React.ReactNode; hint?: string; children: React.ReactNode }) {
  return (
    <div className="set-row">
      <div className="set-label">
        <b>{label}</b>
        {hint && <span>{hint}</span>}
      </div>
      <div style={{ minWidth: 0 }}>{children}</div>
    </div>
  );
}

function CodeBox({ text, onCopy }: { text: string; onCopy: (t: string) => void }) {
  return (
    <div className="codebox">
      <code>{text}</code>
      <button className="btn ghost icon sm" title="Copy" onClick={() => onCopy(text)}>
        <Copy size={13} />
      </button>
    </div>
  );
}
