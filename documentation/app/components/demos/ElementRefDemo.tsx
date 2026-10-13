"use client";

import { useRef, useState } from "react";

export default function ElementRefDemo() {
  const [w, setW] = useState(220);
  const ref = useRef<HTMLDivElement>(null);

  return (
    <div className="flex w-full max-w-112 flex-col items-center gap-4">
      <div ref={ref} style={{ width: w }} className="flex gap-2">
        <div className="flex h-16 flex-1 items-center justify-center rounded bg-[#2B3A4D] text-xs text-white">
          {w}px
        </div>
        <div className="h-16 w-16 shrink-0 rounded bg-[#CA6D60]" />
      </div>
      <input
        type="range"
        min={140}
        max={480}
        value={w}
        onChange={(e) => setW(Number(e.target.value))}
        className="w-56"
      />
      <span className="font-mono text-[10px] text-[#616161]">
        child_rects → [{w}, 64]
      </span>
    </div>
  );
}
