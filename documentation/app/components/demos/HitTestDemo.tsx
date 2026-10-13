"use client";

import { useState } from "react";

export default function HitTestDemo() {
  const [log, setLog] = useState<string[]>([]);

  const hit = (who: string) => setLog((l) => [who, ...l].slice(0, 3));

  return (
    <div className="flex flex-col items-center gap-3">
      <div
        onPointerDown={() => hit("parent")}
        className="relative flex h-28 w-60 cursor-pointer items-center justify-center rounded-lg bg-[#2B3A4D]"
      >
        <span className="text-xs text-white/70">parent · opaque</span>
        <div
          onPointerDown={(e) => e.stopPropagation()}
          className="pointer-events-none absolute right-2 bottom-2 flex h-10 w-16 items-center justify-center rounded bg-[#CA6D60]"
        >
          <span className="text-[9px] text-white">passthrough</span>
        </div>
      </div>
      <div className="flex h-4 items-center gap-2">
        {log.length === 0 ? (
          <span className="text-[10px] text-[#616161]">
            click the boxes — the inner one passes through to the parent
          </span>
        ) : (
          log.map((l, i) => (
            <span key={i} className="font-mono text-[10px] text-[#616161]">
              {l} hit
            </span>
          ))
        )}
      </div>
    </div>
  );
}
