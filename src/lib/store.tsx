// App-wide state: navigation, cached library data and toasts.

import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { call, onChanged, onUpdate, pendingUpdate, type UpdateEvent, type AgentInfo, type Group, type SkillSummary, type Status } from "./api";

export type View = "library" | "insights" | "health" | "agents" | "sync" | "settings";

export interface LibraryFilter {
  group: string | null;
  kind: "all" | "managed" | "unmanaged" | "external" | "archived" | "starred";
  query: string;
}

interface Toast {
  id: number;
  text: string;
  tone: "ok" | "error";
}

interface Store {
  view: View;
  go: (v: View) => void;
  selected: string | null;
  select: (name: string | null) => void;
  /** Jump to a skill from anywhere. */
  openSkill: (name: string) => void;
  filter: LibraryFilter;
  setFilter: (f: Partial<LibraryFilter>) => void;

  status: Status | null;
  skills: SkillSummary[];
  groups: Group[];
  agents: AgentInfo[];
  loaded: boolean;
  /** Bumped whenever data changes; views re-fetch their own data on it. */
  version: number;
  reload: () => Promise<void>;

  toast: (text: string, tone?: "ok" | "error") => void;
  /** Run an API call, toast errors, reload after. Returns the result or undefined. */
  run: <T = unknown>(method: string, params?: Record<string, unknown>, success?: string) => Promise<T | undefined>;

  /** Desktop updates: a version installed and waiting for a restart, or one available to install. */
  update: UpdateEvent;
  setUpdate: (u: UpdateEvent) => void;

  palette: boolean;
  setPalette: (open: boolean) => void;
  newSkill: boolean;
  setNewSkill: (open: boolean) => void;
}

const Ctx = createContext<Store | null>(null);

export function useStore(): Store {
  const s = useContext(Ctx);
  if (!s) throw new Error("useStore outside provider");
  return s;
}

export function StoreProvider({ children }: { children: ReactNode }) {
  const [view, setView] = useState<View>("library");
  const [selected, setSelected] = useState<string | null>(null);
  const [filter, setFilterState] = useState<LibraryFilter>({ group: null, kind: "all", query: "" });
  const [status, setStatus] = useState<Status | null>(null);
  const [skills, setSkills] = useState<SkillSummary[]>([]);
  const [groups, setGroups] = useState<Group[]>([]);
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [version, setVersion] = useState(0);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [palette, setPalette] = useState(false);
  const [newSkill, setNewSkill] = useState(false);
  const [update, setUpdate] = useState<UpdateEvent>({});
  const inflight = useRef<Promise<void> | null>(null);

  const toast = useCallback((text: string, tone: "ok" | "error" = "ok") => {
    const id = Date.now() + Math.random();
    setToasts((t) => [...t, { id, text, tone }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), tone === "error" ? 6000 : 3200);
  }, []);

  const reload = useCallback(async () => {
    if (inflight.current) return inflight.current;
    inflight.current = (async () => {
      try {
        const [st, sk, gr, ag] = await Promise.all([
          call<Status>("status"),
          call<SkillSummary[]>("list_skills"),
          call<Group[]>("list_groups"),
          call<AgentInfo[]>("agents"),
        ]);
        setStatus(st);
        setSkills(sk);
        setGroups(gr);
        setAgents(ag);
        setVersion((v) => v + 1);
      } catch (e) {
        toast(String(e), "error");
      } finally {
        setLoaded(true);
        inflight.current = null;
      }
    })();
    return inflight.current;
  }, [toast]);

  const run = useCallback(
    async <T,>(method: string, params: Record<string, unknown> = {}, success?: string): Promise<T | undefined> => {
      try {
        const out = await call<T>(method, params);
        if (success) toast(success);
        await reload();
        return out;
      } catch (e) {
        toast(String(e).replace(/^Error:\s*/, ""), "error");
        return undefined;
      }
    },
    [reload, toast],
  );

  useEffect(() => {
    pendingUpdate().then((v) => v && setUpdate({ pending: v }), () => {});
    return onUpdate((e) => {
      if (e.error) toast(`Update failed: ${e.error}`, "error");
      else setUpdate(e);
    });
  }, [toast]);

  useEffect(() => {
    reload();
    let t: ReturnType<typeof setTimeout> | undefined;
    const off = onChanged(() => {
      clearTimeout(t);
      t = setTimeout(reload, 250);
    });
    return () => {
      off();
      clearTimeout(t);
    };
  }, [reload]);

  const value = useMemo<Store>(
    () => ({
      view,
      go: setView,
      selected,
      select: setSelected,
      openSkill: (name) => {
        setView("library");
        setSelected(name);
      },
      filter,
      setFilter: (f) => setFilterState((prev) => ({ ...prev, ...f })),
      status,
      skills,
      groups,
      agents,
      loaded,
      version,
      reload,
      toast,
      run,
      update,
      setUpdate,
      palette,
      setPalette,
      newSkill,
      setNewSkill,
    }),
    [view, selected, filter, status, skills, groups, agents, loaded, version, reload, toast, run, update, palette, newSkill],
  );

  return (
    <Ctx.Provider value={value}>
      {children}
      <div className="toasts">
        {toasts.map((t) => (
          <div key={t.id} className={`toast ${t.tone === "error" ? "error" : ""}`}>
            <span className={`sev ${t.tone === "error" ? "error" : "info"}`} style={{ marginTop: 6, background: t.tone === "error" ? undefined : "var(--accent)", boxShadow: t.tone === "error" ? undefined : "0 0 0 3px var(--accent-soft)" }} />
            <span className="selectable" style={{ cursor: "default" }}>{t.text}</span>
          </div>
        ))}
      </div>
    </Ctx.Provider>
  );
}
