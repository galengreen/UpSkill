import { AGENT_META, agentName } from "../../components/ui";

/** Compact agent indicator for dense rows. */
export function AgentDots({ agents }: { agents: string[] }) {
  if (agents.includes("*")) return <span className="lib-dots-all">All agents</span>;
  return (
    <span className="lib-dots" title={agents.map(agentName).join(", ")}>
      {agents.map((a) => (
        <span key={a} className="lib-dot" style={{ background: AGENT_META[a]?.color ?? "var(--text-3)" }} />
      ))}
    </span>
  );
}
