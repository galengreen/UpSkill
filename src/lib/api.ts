// Typed client for the single `api` bridge. Inside Tauri it uses IPC; in a
// plain browser it talks to `upskill serve` (dev only).

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
const DEV_BRIDGE = "http://127.0.0.1:7777/api/";

export async function call<T = unknown>(method: string, params: Record<string, unknown> = {}): Promise<T> {
  if (inTauri) return invoke<T>("api", { method, params });
  const res = await fetch(DEV_BRIDGE + method, { method: "POST", body: JSON.stringify(params) });
  const body = await res.json();
  if (!res.ok) throw new Error(body.error ?? res.statusText);
  return body as T;
}

/** Fires when the library changes on disk (agent edits, MCP calls, sync). */
export function onChanged(cb: () => void): () => void {
  if (!inTauri) return () => {};
  let unlisten: (() => void) | undefined;
  let cancelled = false;
  listen("upskill://changed", cb).then((u) => (cancelled ? u() : (unlisten = u)));
  return () => {
    cancelled = true;
    unlisten?.();
  };
}

// ----------------------------------------------------------------- types

export type Severity = "error" | "warn" | "info";
export type SkillKind = "managed" | "unmanaged" | "link" | "system" | "plugin" | "builtin";

export interface SkillUsage {
  total: number;
  recent: number;
  last_ts: number | null;
  by_agent: Record<string, number>;
  manual: number;
  spark: number[];
}

export interface SkillSummary {
  name: string;
  kind: SkillKind;
  description: string;
  groups: string[];
  /** Agent ids, or ["*"] for every agent. */
  agents: string[];
  inherited: boolean;
  machines: string[] | null;
  archived: boolean;
  starred: boolean;
  deployed_here: boolean;
  updated_ts: number;
  path: string;
  source: string | null;
  tokens: number;
  errors: number;
  warnings: number;
  usage: SkillUsage;
  open_notes: number;
}

export interface Issue {
  code: string;
  severity: Severity;
  message: string;
  file: string | null;
  line: number | null;
}

export interface Placement {
  dirs: string[];
  leaks: string[];
  unreachable: string[];
  duplicates: string[];
}

export interface SkillMeta {
  head: string | null;
  head_ts: number;
  groups: string[];
  agents: string[] | null;
  machines: string[] | null;
  archived: boolean;
  deleted: boolean;
  starred: boolean;
  created_ts: number;
  meta_ts: number;
  source: { kind: string; url: string; subpath: string; upstream_tree: string; imported_ts: number } | null;
}

export interface Note {
  id: string;
  skill: string;
  text: string;
  author: string;
  ts: number;
  resolved: boolean;
}

export interface UsageEvent {
  agent: string;
  skill: string;
  ts: number;
  mode: "auto" | "manual" | "reported";
  cwd: string;
}

export interface SkillDetail {
  name: string;
  kind: "managed";
  path: string;
  meta: SkillMeta;
  content: string;
  description: string;
  frontmatter: Record<string, unknown> | null;
  files: { path: string; size: number; text: boolean }[];
  issues: Issue[];
  usage: SkillUsage;
  recent: UsageEvent[];
  agents: string[];
  inherited: boolean;
  placement: Placement;
  deployed_here: boolean;
  deps: string[];
  dependents: string[];
  versions: number;
  notes: Note[];
  description_tokens: number;
  body_tokens: number;
  overlaps: [string, number][];
}

export interface HistoryEntry {
  id: string;
  ts: number;
  /** "you", "agent:<client>", "external", "sync", "adopt", "import", "upstream", "upskill" */
  author: string;
  machine: string;
  message: string;
  parents: string[];
  tokens: number;
  added: number;
  removed: number;
  is_head: boolean;
}

export interface FileDiff {
  path: string;
  status: "added" | "removed" | "modified";
  diff: string;
  added: number;
  removed: number;
}

export interface Group {
  id: string;
  name: string;
  color: string;
  description: string;
  agents: string[] | null;
  machines: string[] | null;
  order: number;
  count: number;
}

export interface AgentDir {
  path: string;
  readers: string[];
  exists: boolean;
  managed: number;
  unmanaged: number;
  broken: number;
}

export interface AgentInfo {
  id: string;
  name: string;
  color: string;
  installed: boolean;
  managed: number;
  unmanaged: number;
  external: number;
  dirs: AgentDir[];
  mcp: { installed: boolean; config: string } | null;
  budget: number | null;
}

export interface Status {
  machine: { id: string; name: string; os: string };
  onboarded: boolean;
  library: string;
  skills: number;
  archived: number;
  unmanaged: number;
  broken: number;
  external: number;
  agents: string[];
  open_notes: number;
  sync: { kind: "folder" | "git" | null; last: number | null; error: string | null };
  author: string;
}

export interface HealthIssue {
  skill: string | null;
  agent: string | null;
  code: string;
  severity: Severity;
  title: string;
  detail: string;
  /** API method that fixes it: "adopt" | "cleanup_broken" | "set_archived" | "diff" */
  action: string | null;
}

export interface Budget {
  agent: string;
  name: string;
  color: string;
  used: number;
  budget: number;
  skills: number;
  note: string;
  at_risk: string[];
}

export interface HealthReport {
  score: number;
  issues: HealthIssue[];
  budgets: Budget[];
  overlaps: [string, string, number][];
}

export interface UsageReport {
  days: number;
  timeline: { day: string; agents: Record<string, number> }[];
  top: { skill: string; usage: SkillUsage }[];
  recent: UsageEvent[];
}

export interface FoundSkill {
  name: string;
  path: string;
  dir: string;
  kind: "managed" | "unmanaged" | "link" | "broken" | "system" | "plugin";
  readers: string[];
  target: string | null;
  description: string | null;
  tree: string | null;
  source: string | null;
}

export interface MachineInfo {
  id: string;
  name: string;
  os: string;
  last_seen: number;
  agents: string[];
  deployed: number;
}

export interface SyncStatus {
  config: { kind: "folder" | "git"; path: string; remote: string } | null;
  last_sync: number | null;
  error: string | null;
  machines: MachineInfo[];
  this_machine: string;
}

export interface Config {
  machine_id: string;
  machine_name: string;
  budgets: Record<string, number>;
  dark_days: number;
  auto_sync_minutes: number;
  onboarded: boolean;
}

export interface EventEntry {
  ts: number;
  kind: string;
  skill: string | null;
  message: string;
}
