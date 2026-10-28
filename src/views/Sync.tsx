import { useState } from "react";
import { Apple, FolderSync, GitBranch, History, Laptop, Loader2, RefreshCw, ShieldCheck, Terminal, Unplug, Users } from "lucide-react";
import "./pages.css";
import { call, inTauri, type EventEntry, type MachineInfo, type SyncStatus } from "../lib/api";
import { useStore } from "../lib/store";
import { AgentChip, ago, Badge, fmtDate } from "../components/ui";
import { useApi } from "./pages/useApi";

export function Sync() {
  const { data } = useApi<SyncStatus>("sync_status");
  return (
    <div className="page">
      <div className="page-inner">
        <div className="page-head">
          <div>
            <h1 className="page-title">Sync</h1>
            <div className="page-sub">Your library on every machine, without git, merge conflicts or scripts you forget about.</div>
          </div>
        </div>
        {!data ? null : data.config ? <Connected s={data} /> : <Setup />}
      </div>
    </div>
  );
}

function Why() {
  return (
    <div className="card why">
      <div>
        <b>
          <ShieldCheck size={14} style={{ color: "var(--accent)" }} /> Never conflicts
        </b>
        <p>Each machine only writes its own file, plus immutable snapshots. Two machines never touch the same file, so Dropbox and git never see a conflict.</p>
      </div>
      <div>
        <b>
          <History size={14} style={{ color: "var(--accent)" }} /> Nothing is lost
        </b>
        <p>If the same skill is edited on two machines at once, the newest edit wins and the other stays in history, one click from being restored.</p>
      </div>
      <div>
        <b>
          <Users size={14} style={{ color: "var(--accent)" }} /> Per-machine reach
        </b>
        <p>Every skill syncs everywhere so you can edit it anywhere, but only deploys to the machines and agents you choose.</p>
      </div>
    </div>
  );
}

function Setup() {
  const { run } = useStore();
  const [path, setPath] = useState("");
  const [remote, setRemote] = useState("");
  const [busy, setBusy] = useState<string | null>(null);

  const pick = async () => {
    if (!inTauri) return;
    const { open } = await import("@tauri-apps/plugin-dialog");
    const dir = await open({ directory: true, title: "Choose a synced folder" });
    if (typeof dir === "string") setPath(dir);
  };
  const go = async (key: string, method: string, params: Record<string, unknown>, msg: string) => {
    setBusy(key);
    const ok = await run(method, params, msg);
    if (ok) await run("sync_now", {}, "First sync complete");
    setBusy(null);
  };

  return (
    <>
      <div className="pg-grid cols-2">
        <div className="card option-card">
          <div className="opt-icon">
            <FolderSync size={19} />
          </div>
          <h3>Shared folder</h3>
          <p>Point UpSkill at any folder your machines already share: Syncthing, Dropbox, iCloud Drive, Google Drive or a NAS. Best if you already run one of these.</p>
          <div className="opt-foot">
            <div className="row">
              <input className="input mono" style={{ fontSize: 12 }} placeholder="~/Dropbox/UpSkill" value={path} onChange={(e) => setPath(e.target.value)} />
              {inTauri && (
                <button className="btn" onClick={pick}>
                  Browse…
                </button>
              )}
            </div>
            <button className="btn" disabled={!path.trim() || !!busy} onClick={() => go("folder", "configure_sync", { kind: "folder", path: path.trim() }, "Folder sync connected")}>
              {busy === "folder" ? <Loader2 size={14} className="spin" /> : <FolderSync size={14} />} Use this folder
            </button>
          </div>
        </div>

        <div className="card option-card">
          <div className="opt-icon">
            <GitBranch size={19} />
          </div>
          <div className="row">
            <h3>Private GitHub repo</h3>
            <Badge tone="accent">Recommended</Badge>
          </div>
          <p>Works anywhere you have internet, including Omarchy. UpSkill does the git work out of sight; you'll never see a commit, branch or conflict. Needs the gh CLI.</p>
          <div className="opt-foot">
            <button className="btn primary" disabled={!!busy} onClick={() => go("gh", "create_github_repo", { name: "upskill-library" }, "Created a private repo")}>
              {busy === "gh" ? <Loader2 size={14} className="spin" /> : <GitBranch size={14} />} Create a private repo for me
            </button>
            <div className="or">or use an existing one</div>
            <div className="row">
              <input className="input mono" style={{ fontSize: 12 }} placeholder="you/upskill-library" value={remote} onChange={(e) => setRemote(e.target.value)} />
              <button className="btn" disabled={!remote.trim() || !!busy} onClick={() => go("git", "configure_sync", { kind: "git", remote: remote.trim() }, "Repo sync connected")}>
                {busy === "git" ? <Loader2 size={14} className="spin" /> : "Connect"}
              </button>
            </div>
          </div>
        </div>
      </div>
      <div style={{ marginTop: 14 }}>
        <Why />
      </div>
      <div className="faint" style={{ marginTop: 14, fontSize: 12 }}>
        Set it up the same way on each machine. Once connected, every machine appears here as soon as it syncs.
      </div>
    </>
  );
}

function Connected({ s }: { s: SyncStatus }) {
  const { run, toast } = useStore();
  const [busy, setBusy] = useState(false);
  const events = useApi<EventEntry[]>("events", { limit: 100 });
  const syncEvents = (events.data ?? []).filter((e) => e.kind.startsWith("sync")).slice(0, 8);

  const syncNow = async () => {
    setBusy(true);
    const r = await run<{ machines: string[]; updated: string[]; merged: string[]; removed: string[] }>("sync_now");
    if (r) {
      const parts = [
        r.updated.length && `${r.updated.length} updated`,
        r.merged.length && `${r.merged.length} merged`,
        r.removed.length && `${r.removed.length} removed`,
      ].filter(Boolean);
      toast(parts.length ? `Synced: ${parts.join(", ")}` : `Up to date with ${r.machines.length} other machine${r.machines.length === 1 ? "" : "s"}`);
    }
    setBusy(false);
  };
  const disconnect = async () => {
    if (!confirm("Stop syncing this machine? Your library stays as it is, and nothing is deleted.")) return;
    await call("configure_sync", { kind: "none" });
    await run("status", {}, "Sync disconnected");
  };
  const c = s.config!;
  const machines = [...s.machines].sort((a, b) => (a.id === s.this_machine ? -1 : b.id === s.this_machine ? 1 : b.last_seen - a.last_seen));

  return (
    <>
      <div className="card card-pad">
        <div className="row" style={{ alignItems: "flex-start", gap: 16 }}>
          <div className="opt-icon" style={{ width: 38, height: 38, borderRadius: 10, display: "grid", placeItems: "center", background: "var(--accent-soft)", color: "var(--accent)" }}>
            {c.kind === "git" ? <GitBranch size={18} /> : <FolderSync size={18} />}
          </div>
          <div className="grow">
            <div style={{ fontSize: 14, fontWeight: 600 }}>{c.kind === "git" ? "Private repo" : "Shared folder"}</div>
            <dl className="kv" style={{ marginTop: 10 }}>
              <dt>{c.kind === "git" ? "Repository" : "Folder"}</dt>
              <dd className="mono ellipsis selectable" style={{ fontSize: 11.5 }}>
                {c.kind === "git" ? c.remote : c.path}
              </dd>
              <dt>Last sync</dt>
              <dd>{s.last_sync ? `${ago(s.last_sync)} · ${fmtDate(s.last_sync)}` : "Not yet"}</dd>
              {s.error && (
                <>
                  <dt style={{ color: "var(--error)" }}>Error</dt>
                  <dd className="selectable" style={{ color: "var(--error)" }}>
                    {s.error}
                  </dd>
                </>
              )}
            </dl>
          </div>
          <div className="row">
            <button className="btn ghost sm danger" onClick={disconnect}>
              <Unplug size={13} /> Disconnect
            </button>
            <button className="btn primary" disabled={busy} onClick={syncNow}>
              <RefreshCw size={14} className={busy ? "spin" : undefined} /> Sync now
            </button>
          </div>
        </div>
      </div>

      <div className="pg-section">
        <div className="pg-section-head">
          <h2>Machines</h2>
          <p>Everything that shares this library.</p>
        </div>
        <div className="card">
          {machines.map((m) => (
            <Machine key={m.id} m={m} me={m.id === s.this_machine} />
          ))}
          {machines.length < 2 && (
            <div className="faint" style={{ padding: "12px 18px", borderTop: "1px solid var(--line)", fontSize: 12 }}>
              Install UpSkill on your other machines and connect them to the same {c.kind === "git" ? "repo" : "folder"}. They'll appear here after their first sync.
            </div>
          )}
        </div>
      </div>

      {syncEvents.length > 0 && (
        <div className="pg-section">
          <div className="pg-section-head">
            <h2>Recent merges</h2>
            <p>Concurrent edits that sync resolved for you.</p>
          </div>
          <div className="card">
            {syncEvents.map((e, i) => (
              <div className="event" key={i}>
                <span className="faint">{ago(e.ts)}</span>
                <span className="faint mono" style={{ fontSize: 11 }}>
                  {e.kind}
                </span>
                <span>{e.message}</span>
              </div>
            ))}
          </div>
        </div>
      )}

      <div className="pg-section">
        <Why />
      </div>
    </>
  );
}

function Machine({ m, me }: { m: MachineInfo; me: boolean }) {
  const Icon = m.os === "macos" ? Apple : m.os === "linux" ? Terminal : Laptop;
  return (
    <div className="machine">
      <div className="machine-icon">
        <Icon size={16} />
      </div>
      <div style={{ minWidth: 0 }}>
        <div className="row">
          <span style={{ fontWeight: 500 }}>{m.name}</span>
          {me && <Badge tone="accent">This machine</Badge>}
        </div>
        <div className="faint" style={{ fontSize: 11.5, marginTop: 2 }}>
          {m.os === "macos" ? "macOS" : m.os === "linux" ? "Linux" : m.os} · {m.deployed} skills deployed · {me ? "online" : `seen ${ago(m.last_seen)}`}
        </div>
      </div>
      <span className="row" style={{ gap: 5 }}>
        {m.agents.map((a) => (
          <AgentChip key={a} id={a} />
        ))}
      </span>
      <span />
    </div>
  );
}
