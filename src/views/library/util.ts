import type { SkillSummary } from "../../lib/api";
import type { LibraryFilter } from "../../lib/store";

export type Sort = "used" | "recent" | "az";
export type DetailTab = "edit" | "reach" | "history" | "usage" | "notes";

export const EXTERNAL_KINDS = ["system", "plugin", "builtin"];

export const KIND_LABEL: Record<string, string> = {
  managed: "Managed",
  unmanaged: "Unmanaged",
  link: "Linked",
  system: "Built-in",
  plugin: "Plugin",
  builtin: "Bundled",
};

export function filterSkills(skills: SkillSummary[], f: LibraryFilter, sort: Sort): SkillSummary[] {
  const q = f.query.trim().toLowerCase();
  const out = skills.filter((s) => {
    if (f.group && !s.groups.includes(f.group)) return false;
    switch (f.kind) {
      case "all":
        if (s.archived) return false;
        break;
      case "managed":
        if (s.kind !== "managed" || s.archived) return false;
        break;
      case "unmanaged":
        if (s.kind !== "unmanaged" && s.kind !== "link") return false;
        break;
      case "external":
        if (!EXTERNAL_KINDS.includes(s.kind)) return false;
        break;
      case "starred":
        if (!s.starred) return false;
        break;
      case "archived":
        if (!s.archived) return false;
        break;
    }
    return !q || s.name.toLowerCase().includes(q) || s.description.toLowerCase().includes(q);
  });
  const managedFirst = (a: SkillSummary, b: SkillSummary) => Number(b.kind === "managed") - Number(a.kind === "managed");
  out.sort((a, b) => {
    if (sort === "az") return a.name.localeCompare(b.name);
    if (sort === "recent") return b.updated_ts - a.updated_ts || managedFirst(a, b) || a.name.localeCompare(b.name);
    return b.usage.recent - a.usage.recent || b.usage.total - a.usage.total || managedFirst(a, b) || a.name.localeCompare(b.name);
  });
  return out;
}

/** Turn free text into a valid skill name as the user types. */
export function slugify(s: string): string {
  return s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-/, "")
    .slice(0, 64);
}

export function validName(n: string): boolean {
  return n.length > 0 && n.length <= 64 && /^[a-z0-9]+(-[a-z0-9]+)*$/.test(n);
}

export function titleCase(name: string): string {
  return name
    .split(/[-_]/)
    .filter(Boolean)
    .map((w) => w[0].toUpperCase() + w.slice(1))
    .join(" ");
}

/** Pull `description` out of SKILL.md frontmatter (handles block and multi-line scalars). */
export function descriptionOf(content: string): string | null {
  const m = /^﻿?---\r?\n([\s\S]*?)\r?\n---/.exec(content);
  if (!m) return null;
  const lines = m[1].split(/\r?\n/);
  const i = lines.findIndex((l) => /^description:/.test(l));
  if (i < 0) return null;
  let v = lines[i].replace(/^description:\s*/, "");
  if (/^[>|][-+]?\s*$/.test(v)) {
    const folded = v.startsWith(">");
    const rest: string[] = [];
    for (let j = i + 1; j < lines.length && (/^\s/.test(lines[j]) || lines[j] === ""); j++) rest.push(lines[j].trim());
    return rest.join(folded ? " " : "\n").trim();
  }
  const rest = [v];
  for (let j = i + 1; j < lines.length && /^\s+\S/.test(lines[j]); j++) rest.push(lines[j].trim());
  v = rest.join(" ").trim();
  if ((v.startsWith('"') && v.endsWith('"')) || (v.startsWith("'") && v.endsWith("'"))) v = v.slice(1, -1);
  return v;
}

/** Cross-component nudges (palette → library) without growing the global store. */
export function emit(name: "upskill:import-repo" | "upskill:new-group" | "upskill:tab", detail?: unknown) {
  window.dispatchEvent(new CustomEvent(name, { detail }));
}

export function shortPath(p: string): string {
  const home = /^\/(Users|home)\/[^/]+/;
  const s = p.replace(home, "~");
  const parts = s.split("/");
  return parts.length > 4 ? `…/${parts.slice(-3).join("/")}` : s;
}

export function plural(n: number, word: string, many = `${word}s`): string {
  return `${n} ${n === 1 ? word : many}`;
}
