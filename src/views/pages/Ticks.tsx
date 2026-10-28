import { Fragment } from "react";

/** Render `backtick` spans in lint/health messages as inline code. */
export function Ticks({ text }: { text: string }) {
  const parts = text.split(/`([^`]+)`/g);
  return (
    <>
      {parts.map((p, i) =>
        i % 2 ? (
          <code key={i} style={{ padding: "1px 5px", borderRadius: 5, background: "var(--bg-3)", color: "var(--text)", fontSize: "0.88em" }}>
            {p}
          </code>
        ) : (
          <Fragment key={i}>{p}</Fragment>
        ),
      )}
    </>
  );
}
