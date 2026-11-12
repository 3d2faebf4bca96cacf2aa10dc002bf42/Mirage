import { useEffect, useRef } from "react";
import type { LogLine } from "../types";

type Props = { logs: LogLine[] };

export function EventConsole({ logs }: Props) {
  const endRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ block: "end" });
  }, [logs.length]);

  return (
    <section className="panel panel--console">
      <div className="panel__head">
        <h2 className="panel__title">Events</h2>
        <span className="count">{logs.length}</span>
      </div>
      <div className="console" role="log" aria-live="polite">
        {logs.map((line, index) => (
          <p key={`${line.ts}-${index}`} className={`console__line console__line--${levelClass(line.level)}`}>
            <span className="console__ts">[{line.ts}]</span> {line.text}
          </p>
        ))}
        {logs.length === 0 && <p className="console__line muted">waiting for events…</p>}
        <div ref={endRef} />
      </div>
    </section>
  );
}

function levelClass(level: string): string {
  switch (level) {
    case "ok":
      return "ok";
    case "error":
      return "error";
    case "warn":
      return "warn";
    default:
      return "info";
  }
}
