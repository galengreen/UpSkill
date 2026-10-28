import { useState } from "react";
import { useStore } from "../../lib/store";
import { Modal } from "../../components/ui";
import { slugify, validName } from "./util";

export function ImportRepoModal({ onClose }: { onClose: () => void }) {
  const s = useStore();
  const [url, setUrl] = useState("");
  const [busy, setBusy] = useState(false);
  const install = async () => {
    if (!url.trim() || busy) return;
    setBusy(true);
    const r = await s.run<{ imported: string[] }>("import_repo", { url: url.trim() });
    setBusy(false);
    if (!r) return;
    const added = r.imported.filter((x) => !x.includes(":"));
    const skipped = r.imported.length - added.length;
    s.toast(added.length ? `Installed ${added.length} skill${added.length === 1 ? "" : "s"}${skipped ? `, ${skipped} already in your library` : ""}` : "Everything in that repo is already in your library");
    if (added[0]) s.openSkill(added[0]);
    onClose();
  };
  return (
    <Modal
      title="Install from a repo"
      subtitle="Skills are copied into your library and keep a link to their source, so you can pull upstream updates without losing your edits."
      onClose={onClose}
      footer={
        <>
          <button className="btn ghost" onClick={onClose}>
            Cancel
          </button>
          <button className="btn primary" disabled={!url.trim() || busy} onClick={install}>
            {busy ? "Fetching…" : "Install"}
          </button>
        </>
      }
    >
      <div>
        <label className="label">Repository</label>
        <input
          className="input mono"
          autoFocus
          placeholder="owner/repo or https://github.com/owner/repo"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && install()}
          spellCheck={false}
        />
        <div className="hint">
          For example <span className="mono" style={{ color: "var(--text-2)", cursor: "pointer" }} onClick={() => setUrl("t3dotgg/tstack")}>t3dotgg/tstack</span>. Every folder with a SKILL.md is picked up.
        </div>
      </div>
    </Modal>
  );
}

export function RenameModal({ name, onClose }: { name: string; onClose: () => void }) {
  const s = useStore();
  const [value, setValue] = useState(name);
  const clean = value.replace(/-+$/, "");
  const taken = clean !== name && s.skills.some((x) => x.name === clean);
  const ok = validName(clean) && clean !== name && !taken;
  const submit = async () => {
    if (!ok) return;
    const r = await s.run("rename_skill", { name, new_name: clean }, `Renamed to ${clean}`);
    if (r !== undefined) {
      s.select(clean);
      onClose();
    }
  };
  return (
    <Modal
      title="Rename skill"
      subtitle="The folder, the frontmatter and every agent's link are updated together."
      onClose={onClose}
      footer={
        <>
          <button className="btn ghost" onClick={onClose}>
            Cancel
          </button>
          <button className="btn primary" disabled={!ok} onClick={submit}>
            Rename
          </button>
        </>
      }
    >
      <div>
        <input className="input mono" autoFocus value={value} onChange={(e) => setValue(slugify(e.target.value))} onKeyDown={(e) => e.key === "Enter" && submit()} spellCheck={false} />
        {taken && <div className="field-err">There's already a skill called {clean}.</div>}
      </div>
    </Modal>
  );
}

export function ConfirmModal({ title, body, confirm, onConfirm, onClose }: { title: string; body: string; confirm: string; onConfirm: () => Promise<void> | void; onClose: () => void }) {
  const [busy, setBusy] = useState(false);
  return (
    <Modal
      title={title}
      onClose={onClose}
      footer={
        <>
          <button className="btn ghost" onClick={onClose}>
            Cancel
          </button>
          <button
            className="btn danger"
            disabled={busy}
            autoFocus
            onClick={async () => {
              setBusy(true);
              await onConfirm();
              onClose();
            }}
          >
            {confirm}
          </button>
        </>
      }
    >
      <p style={{ margin: 0, color: "var(--text-2)" }}>{body}</p>
    </Modal>
  );
}
