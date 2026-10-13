"use client";

import { useState } from "react";

const MIN_CHILD = 64;
const GAP = 8;
const COUNT = 12;

export default function AutoGridDemo() {
  const [width, setWidth] = useState(300);
  const cols = Math.max(1, Math.floor((width + GAP) / (MIN_CHILD + GAP)));

  return (
    <div className="flex w-full max-w-112 flex-col items-center gap-4">
      <div className="flex items-center gap-3">
        <span className="text-xs text-[#616161]">width</span>
        <input
          type="range"
          min={140}
          max={480}
          value={width}
          onChange={(e) => setWidth(Number(e.target.value))}
          className="w-44"
        />
        <span className="w-24 font-mono text-xs tabular-nums text-[#616161]">
          {width}px · {cols} col{cols > 1 ? "s" : ""}
        </span>
      </div>
      <div
        className="overflow-hidden rounded-lg border border-gray-300 bg-white p-2"
        style={{ width }}
      >
        <div
          className="grid gap-2"
          style={{ gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))` }}
        >
          {Array.from({ length: COUNT }, (_, i) => (
            <div key={i} className="h-10 rounded bg-[#2B3A4D]/85" />
          ))}
        </div>
      </div>
    </div>
  );
}
