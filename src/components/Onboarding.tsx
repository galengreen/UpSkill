import { useEffect, useState } from "react";
import { ArrowRight, Check, ChevronsUp, Download, FolderSync, History, Link2Off, Loader2, Plug, ShieldCheck, Sparkles, Waypoints } from "lucide-react";
import "../views/pages.css";
import { call } from "../lib/api";
import { useStore } from "../lib/store";
import { AgentChip } from "./ui";

interface McpStatus {
  bin: string | null;
  targets: { agent: string; config: string; installed: boolean }[];
}

const STEPS = 4;

export function Onboarding() {
  const s = useStore();
  const [step, setStep] = useState(0);
  const [busy, setBusy] = useState<string | null>(null);
  const [done, setDone] = useState<Record<string, boolean>>({});
  const [mcp, setMcp] = useState<McpStatus | null>(null);
  const st = s.status;

  useEffect(() => {
    call<McpStatus>("mcp_status").then(setMcp).catch(() => {});
  }, [s.version]);

  const finish = async (thenSync = false) => {
    await call("settings_set", { onboarded: true });
    await s.reload();
    if (thenSync) s.go("sync");
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && finish();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const act = async (key: string, method: string, params: Record<string, unknown> = {}, msg?: string) => {
    setBusy(key);
    const out = await s.run(method, params, msg);
    if (out !== undefined) setDone((d) => ({ ...d, [key]: true }));
    setBusy(null);
    return out;
  };

  if (!st) return null;
  const installed = st.agents;
  const mcpTargets = (mcp?.targets ?? []).filter((t) => installed.includes(t.agent));
  const hasCompanion = s.skills.some((x) => x.name === "upskill" && x.kind === "managed");

  return (
    <div className="onb-overlay">
      <div className="onb">
        <div className="onb-progress">
          {Array.from({ length: STEPS }, (_, i) => (
            <span key={i} className={i <= step ? "done" : ""} />
          ))}
        </div>
        <div className="onb-body">
          {step === 0 && (
            <div className="onb-step" key="0">
              <div className="onb-glyph">
                <ChevronsUp size={28} strokeWidth={2.4} />
              </div>
              <h1>One library for every agent's skills</h1>
              <p className="lede">
                Claude Code, Codex, Amp, Cursor and friends each keep skills in their own folders, on every machine. UpSkill gives you one versioned library and keeps them all in step: right agents,
                right machines, no copies drifting apart.
              </p>
              <div className="onb-found">
                <div>
                  <b>{st.unmanaged}</b>
                  <span>unmanaged skills in agent folders</span>
                </div>
                <div>
                  <b style={{ color: st.broken ? "var(--warn)" : undefined }}>{st.broken}</b>
                  <span>broken links</span>
                </div>
                <div>
                  <b>{st.external}</b>
                  <span>plugin &amp; built-in skills tracked</span>
                </div>
              </div>
              <div className="row" style={{ marginTop: 2 }}>
                <span className="faint">Agents on this machine</span>
                <span className="row" style={{ marginLeft: "auto", gap: 6 }}>
                  {installed.map((a) => (
                    <span key={a} title={a}>
                      <AgentChip id={a} size={22} />
                    </span>
                  ))}
                </span>
              </div>
            </div>
          )}

          {step === 1 && (
            <div className="onb-step" key="1">
              <div className="onb-glyph">
                <Download size={24} />
              </div>
              <h1>Bring your skills in</h1>
              <p className="lede">Adopting moves each skill into your library and links it back to every agent that could see it before.</p>
              <div className="onb-points">
                <div>
                  <ShieldCheck size={15} /> Originals are backed up to <span className="mono">~/.upskill/backups</span> first.
                </div>
                <div>
                  <History size={15} /> If copies differ between agents, every version goes into history and the newest becomes current.
                </div>
                <div>
                  <Waypoints size={15} /> Reach is preserved: a skill only Codex could see stays Codex-only.
                </div>
              </div>
              <div className="row" style={{ marginTop: "auto", gap: 8 }}>
                <button className="btn primary" disabled={!st.unmanaged || !!busy || done.adopt} onClick={() => act("adopt", "adopt_all", {}, "Skills adopted")}>
                  {busy === "adopt" ? <Loader2 size={14} className="spin" /> : done.adopt ? <Check size={14} /> : <Download size={14} />}
                  {done.adopt ? "Adopted" : st.unmanaged ? `Adopt all ${st.unmanaged}` : "Nothing to adopt"}
                </button>
                {st.broken > 0 && (
                  <button className="btn" disabled={!!busy || done.broken} onClick={() => act("broken", "cleanup_broken", {}, "Broken links removed")}>
                    {busy === "broken" ? <Loader2 size={14} className="spin" /> : done.broken ? <Check size={14} /> : <Link2Off size={14} />}
                    {done.broken ? "Cleaned up" : `Clean up ${st.broken} broken link${st.broken === 1 ? "" : "s"}`}
                  </button>
                )}
              </div>
            </div>
          )}

          {step === 2 && (
            <div className="onb-step" key="2">
              <div className="onb-glyph">
                <Plug size={24} />
              </div>
              <h1>Let agents manage skills</h1>
              <p className="lede">Connect UpSkill's MCP server and your agents can write, fix and organise skills for you. Every edit is versioned and credited to the agent that made it.</p>
              <div>
                {mcpTargets.map((t) => (
                  <div className="onb-agent" key={t.agent}>
                    <AgentChip id={t.agent} size={22} />
                    <span style={{ fontWeight: 500 }}>{s.agents.find((a) => a.id === t.agent)?.name ?? t.agent}</span>
                    <span className="faint mono" style={{ fontSize: 11 }}>
                      {t.config}
                    </span>
                    <span style={{ marginLeft: "auto" }}>
                      {t.installed || done[`mcp-${t.agent}`] ? (
                        <span className="done-tick">
                          <Check size={13} /> Connected
                        </span>
                      ) : (
                        <button className="btn sm" disabled={!!busy || !mcp?.bin} onClick={() => act(`mcp-${t.agent}`, "mcp_install", { agent: t.agent })}>
                          {busy === `mcp-${t.agent}` ? <Loader2 size={13} className="spin" /> : <Plug size={13} />} Connect
                        </button>
                      )}
                    </span>
                  </div>
                ))}
                <div className="onb-agent">
                  <span className="brand-mark" style={{ width: 22, height: 22 }}>
                    <Sparkles size={12} />
                  </span>
                  <span style={{ fontWeight: 500 }}>Companion skill</span>
                  <span className="faint" style={{ fontSize: 11.5 }}>
                    teaches agents to use UpSkill
                  </span>
                  <span style={{ marginLeft: "auto" }}>
                    {hasCompanion || done.companion ? (
                      <span className="done-tick">
                        <Check size={13} /> Added
                      </span>
                    ) : (
                      <button className="btn sm" disabled={!!busy} onClick={() => act("companion", "seed_companion")}>
                        Add
                      </button>
                    )}
                  </span>
                </div>
              </div>
            </div>
          )}

          {step === 3 && (
            <div className="onb-step" key="3">
              <div className="onb-glyph">
                <FolderSync size={24} />
              </div>
              <h1>Take it to your other machines</h1>
              <p className="lede">
                Sync through any folder you already share, or a private GitHub repo UpSkill manages for you. No git commands and no merge conflicts, and a skill can still be limited to one
                machine.
              </p>
              <div className="onb-points">
                <div>
                  <Check size={15} /> Each machine writes only its own file, so syncing never conflicts.
                </div>
                <div>
                  <Check size={15} /> Usage from every machine rolls up into one picture.
                </div>
              </div>
              <div className="row" style={{ marginTop: "auto" }}>
                <button className="btn primary" onClick={() => finish(true)}>
                  <FolderSync size={14} /> Set up sync
                </button>
              </div>
            </div>
          )}
        </div>
        <div className="onb-foot">
          <button className="btn ghost sm" onClick={() => finish()}>
            Skip setup
          </button>
          <span className="faint" style={{ marginLeft: "auto", fontSize: 11.5 }}>
            {step + 1} of {STEPS}
          </span>
          {step > 0 && (
            <button className="btn ghost" onClick={() => setStep(step - 1)}>
              Back
            </button>
          )}
          {step < STEPS - 1 ? (
            <button className="btn primary" onClick={() => setStep(step + 1)}>
              {step === 0 ? "Get started" : "Continue"} <ArrowRight size={14} />
            </button>
          ) : (
            <button className="btn" onClick={() => finish()}>
              Finish
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
