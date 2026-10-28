import { useEffect, useMemo, useRef, useState } from "react";
import { CircleCheck, FileCode, FileText, Zap } from "lucide-react";
import { call, type Issue, type SkillDetail } from "../../lib/api";
import { useStore } from "../../lib/store";
import { Sev } from "../../components/ui";
import { Editor, type EditorHandle } from "../../components/Editor";
import { descriptionOf } from "./util";

const RANK = { error: 0, warn: 1, info: 2 } as const;

export function EditTab({ detail, reload }: { detail: SkillDetail; reload: () => void }) {
  const s = useStore();
  const name = detail.name;
  const [file, setFile] = useState("SKILL.md");
  const [saved, setSaved] = useState(detail.content);
  const [buffer, setBuffer] = useState(detail.content);
  const [editorKey, setEditorKey] = useState(0);
  const [stale, setStale] = useState(false);
  const [issues, setIssues] = useState<Issue[]>(detail.issues);
  const [saving, setSaving] = useState(false);
  const ed = useRef<EditorHandle>(null);
  const dirty = buffer !== saved;
  const isSkillMd = file === "SKILL.md";

  const replace = (content: string) => {
    setSaved(content);
    setBuffer(content);
    setStale(false);
    setEditorKey((k) => k + 1);
  };

  // Follow edits made on disk (agents, other editors) unless there are unsaved changes.
  useEffect(() => {
    if (!isSkillMd || detail.content === saved) return;
    if (buffer === saved) replace(detail.content);
    else setStale(true);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [detail.content]);

  // Live lint while typing.
  useEffect(() => {
    if (!isSkillMd) return;
    const t = setTimeout(() => {
      call<Issue[]>("lint_draft", { name, content: buffer })
        .then(setIssues)
        .catch(() => {});
    }, 350);
    return () => clearTimeout(t);
  }, [buffer, isSkillMd, name]);

  const open = async (path: string) => {
    if (path === file) return;
    if (dirty) {
      s.toast("Save or discard your changes first", "error");
      return;
    }
    if (path === "SKILL.md") {
      setFile(path);
      replace(detail.content);
      return;
    }
    try {
      const r = await call<{ content: string }>("read_skill_file", { name, path });
      setFile(path);
      replace(r.content);
    } catch (e) {
      s.toast(String(e), "error");
    }
  };

  const save = async () => {
    if (!dirty || saving) return;
    setSaving(true);
    const r = isSkillMd
      ? await s.run("update_skill", { name, content: buffer, message: "Edited in UpSkill" }, "Saved")
      : await s.run("write_skill_file", { name, path: file, content: buffer }, "Saved");
    setSaving(false);
    if (r !== undefined) {
      setSaved(buffer);
      setStale(false);
      reload();
    }
  };
  const saveRef = useRef(save);
  saveRef.current = save;

  const sorted = useMemo(() => [...issues].sort((a, b) => RANK[a.severity] - RANK[b.severity]), [issues]);
  const description = useMemo(() => (isSkillMd ? descriptionOf(buffer) : null), [buffer, isSkillMd]);

  return (
    <div className="edit">
      {detail.files.length > 1 && (
        <aside className="edit-files">
          <div className="foot-title" style={{ padding: "2px 8px 6px" }}>
            Files
          </div>
          {detail.files.map((f) => (
            <button key={f.path} className={`edit-file${f.path === file ? " on" : ""}`} disabled={!f.text} title={f.text ? f.path : `${f.path} (binary)`} onClick={() => open(f.path)}>
              {f.path === "SKILL.md" ? <FileText size={12} /> : <FileCode size={12} />}
              <span className="ellipsis">{f.path}</span>
            </button>
          ))}
        </aside>
      )}
      <div className="edit-main">
        <div className="edit-bar">
          <span className="mono edit-file-name">{file}</span>
          {dirty && <span className="dirty-dot" title="Unsaved changes" />}
          <span className="grow" />
          {stale && (
            <span className="edit-stale">
              Changed on disk
              <button className="btn ghost sm" onClick={() => replace(detail.content)}>
                Load theirs
              </button>
            </span>
          )}
          {dirty && (
            <button className="btn ghost sm" onClick={() => replace(saved)}>
              Discard
            </button>
          )}
          <button className="btn sm primary" disabled={!dirty || saving} onClick={save}>
            {saving ? "Saving…" : "Save"} <span className="btn-kbd">⌘S</span>
          </button>
        </div>
        <div className="edit-editor">
          <Editor key={`${file}:${editorKey}`} ref={ed} value={buffer} onChange={setBuffer} onSave={() => saveRef.current()} markdownMode={/\.(md|markdown|txt)$/i.test(file)} />
        </div>
        {isSkillMd && (
          <div className="edit-foot">
            <div className="edit-problems">
              <div className="foot-title">
                Problems {sorted.length > 0 && <span className="num">{sorted.length}</span>}
              </div>
              {sorted.length === 0 ? (
                <div className="problem-clean">
                  <CircleCheck size={14} /> No problems. Clean, lean and ready to trigger.
                </div>
              ) : (
                sorted.map((i, n) => {
                  const jumpable = i.line != null && (!i.file || i.file === "SKILL.md");
                  return (
                    <div key={n} className={`problem${jumpable ? " jump" : ""}`} onClick={() => jumpable && ed.current?.jumpTo(i.line!)}>
                      <Sev s={i.severity} />
                      <span className="grow">{i.message}</span>
                      {(i.line != null || (i.file && i.file !== "SKILL.md")) && (
                        <span className="problem-loc">
                          {i.file && i.file !== "SKILL.md" ? i.file : ""}
                          {i.line != null ? `:${i.line}` : ""}
                        </span>
                      )}
                    </div>
                  );
                })
              )}
            </div>
            <TriggerPreview text={description ?? ""} />
          </div>
        )}
      </div>
    </div>
  );
}

function TriggerPreview({ text }: { text: string }) {
  const chars = [...text];
  const len = chars.length;
  const head = chars.slice(0, 250).join("");
  const tail = chars.slice(250).join("");
  return (
    <div className="trigger">
      <div className="foot-title">
        <Zap size={11} /> What agents see
        <span className="grow" />
        <span className={`num trigger-count${len > 1024 ? " over" : len > 250 ? " long" : ""}`}>{len}/1024</span>
      </div>
      <p className="trigger-text selectable">
        {text ? (
          <>
            <span className="trigger-hot">{head}</span>
            <span className="trigger-cold">{tail}</span>
          </>
        ) : (
          <span className="faint">No description yet.</span>
        )}
      </p>
      <div className="trigger-hint">Trigger words belong in the highlighted part. Listings get cut, so the faded part may never be read.</div>
    </div>
  );
}
