// CodeMirror 6 editor for SKILL.md and supporting files, themed to match UpSkill.

import { useEffect, useImperativeHandle, useRef, type Ref } from "react";
import { EditorState, RangeSetBuilder, StateField } from "@codemirror/state";
import {
  Decoration,
  drawSelection,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
  type DecorationSet,
} from "@codemirror/view";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { markdown } from "@codemirror/lang-markdown";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { highlightSelectionMatches, searchKeymap } from "@codemirror/search";
import { tags as t } from "@lezer/highlight";

export interface EditorHandle {
  jumpTo: (line: number) => void;
  focus: () => void;
}

const theme = EditorView.theme(
  {
    "&": { height: "100%", fontSize: "12.5px", backgroundColor: "transparent", color: "var(--text)" },
    ".cm-scroller": { fontFamily: "var(--mono)", lineHeight: "1.7" },
    ".cm-content": { padding: "14px 0 40px", caretColor: "var(--accent)" },
    ".cm-gutters": { backgroundColor: "transparent", border: "none", color: "var(--text-4)" },
    ".cm-lineNumbers .cm-gutterElement": { padding: "0 10px 0 18px", minWidth: "44px", fontSize: "11px" },
    ".cm-activeLine": { backgroundColor: "rgba(255,255,255,0.022)" },
    ".cm-activeLineGutter": { backgroundColor: "transparent", color: "var(--text-2)" },
    ".cm-cursor, .cm-dropCursor": { borderLeftColor: "var(--accent)", borderLeftWidth: "2px" },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection": {
      backgroundColor: "rgba(196,245,108,0.16) !important",
    },
    "&.cm-focused": { outline: "none" },
    ".cm-line": { padding: "0 24px 0 6px" },
    ".cm-selectionMatch": { backgroundColor: "rgba(255,255,255,0.07)" },
    ".cm-searchMatch": { backgroundColor: "rgba(245,184,74,0.18)", outline: "1px solid rgba(245,184,74,0.35)" },
    ".cm-panels": { backgroundColor: "var(--bg-2)", color: "var(--text)", borderTop: "1px solid var(--line)" },
    ".cm-panels input, .cm-panels button": { fontFamily: "var(--font)" },
  },
  { dark: true },
);

const highlight = HighlightStyle.define([
  { tag: [t.heading1, t.heading2], color: "#f6f6f8", fontWeight: "650" },
  { tag: [t.heading3, t.heading4, t.heading5, t.heading6, t.heading], color: "#e4e4ea", fontWeight: "600" },
  { tag: t.strong, color: "#f2f2f4", fontWeight: "650" },
  { tag: t.emphasis, color: "#d6d6dc", fontStyle: "italic" },
  { tag: [t.link, t.url], color: "var(--accent)" },
  { tag: t.monospace, color: "#f0bf6a" },
  { tag: t.quote, color: "var(--text-2)", fontStyle: "italic" },
  { tag: [t.processingInstruction, t.meta, t.contentSeparator, t.punctuation], color: "var(--text-3)" },
  { tag: t.strikethrough, textDecoration: "line-through" },
]);

// YAML frontmatter gets its own quiet styling with highlighted keys.
const fmLine = Decoration.line({ class: "cm-fm" });
const fmDelim = Decoration.line({ class: "cm-fm cm-fm-delim" });
const fmKey = Decoration.mark({ class: "cm-fm-key" });

function frontmatterDecos(state: EditorState): DecorationSet {
  const doc = state.doc;
  if (doc.lines < 2 || doc.line(1).text.trim() !== "---") return Decoration.none;
  let end = -1;
  for (let i = 2; i <= Math.min(doc.lines, 300); i++) {
    if (doc.line(i).text.trim() === "---") {
      end = i;
      break;
    }
  }
  if (end < 0) return Decoration.none;
  const b = new RangeSetBuilder<Decoration>();
  for (let i = 1; i <= end; i++) {
    const l = doc.line(i);
    const delim = i === 1 || i === end;
    b.add(l.from, l.from, delim ? fmDelim : fmLine);
    const m = delim ? null : /^([A-Za-z0-9_-]+):/.exec(l.text);
    if (m) b.add(l.from, l.from + m[1].length + 1, fmKey);
  }
  return b.finish();
}

const frontmatter = StateField.define<DecorationSet>({
  create: frontmatterDecos,
  update: (d, tr) => (tr.docChanged ? frontmatterDecos(tr.state) : d),
  provide: (f) => EditorView.decorations.from(f),
});

/** Uncontrolled: `value` is the initial document. Remount with a `key` to load new content. */
export function Editor({
  value,
  onChange,
  onSave,
  markdownMode = true,
  ref,
}: {
  value: string;
  onChange: (v: string) => void;
  onSave?: () => void;
  markdownMode?: boolean;
  ref?: Ref<EditorHandle>;
}) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const cb = useRef({ onChange, onSave });
  cb.current = { onChange, onSave };

  useEffect(() => {
    const state = EditorState.create({
      doc: value,
      extensions: [
        keymap.of([{ key: "Mod-s", preventDefault: true, run: () => (cb.current.onSave?.(), true) }]),
        lineNumbers(),
        highlightActiveLine(),
        highlightActiveLineGutter(),
        drawSelection(),
        history(),
        highlightSelectionMatches(),
        EditorView.lineWrapping,
        keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap, indentWithTab]),
        ...(markdownMode ? [markdown(), frontmatter] : []),
        syntaxHighlighting(highlight),
        theme,
        EditorView.updateListener.of((u) => {
          if (u.docChanged) cb.current.onChange(u.state.doc.toString());
        }),
      ],
    });
    const v = new EditorView({ state, parent: host.current! });
    view.current = v;
    return () => {
      v.destroy();
      view.current = null;
    };
    // Initial value only; callers remount via `key`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useImperativeHandle(
    ref,
    () => ({
      jumpTo(line) {
        const v = view.current;
        if (!v) return;
        const l = v.state.doc.line(Math.min(Math.max(1, line), v.state.doc.lines));
        v.dispatch({ selection: { anchor: l.from }, effects: EditorView.scrollIntoView(l.from, { y: "center" }) });
        v.focus();
      },
      focus() {
        view.current?.focus();
      },
    }),
    [],
  );

  return <div ref={host} className="cm-host selectable" />;
}
