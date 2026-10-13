"use client";

import { useState } from "react";

export default function SignalsDemo() {
  const [count, setCount] = useState(0);
  const [pressed, setPressed] = useState(false);
  const active = pressed && count % 2 === 0;

  return (
    <div className="flex flex-col items-center gap-5">
      <div className="flex items-center gap-3">
        <button
          onClick={() => setCount((c) => c + 1)}
          className="cursor-pointer rounded bg-[#2563eb] px-4 py-2 text-sm font-semibold text-white select-none"
        >
          increment
        </button>
        <span className="w-20 text-center font-mono text-sm tabular-nums">
          {count}
        </span>
        <button
          onClick={() => setPressed((p) => !p)}
          className={`cursor-pointer rounded px-4 py-2 text-sm font-semibold text-white select-none ${
            pressed ? "bg-[#CA6D60]" : "bg-[#2B3A4D]"
          }`}
        >
          {pressed ? "pressed" : "idle"}
        </button>
      </div>
      <div className="flex items-center gap-2 text-xs text-[#616161]">
        <span>derived active:</span>
        <span
          className={`rounded px-2 py-0.5 font-mono ${
            active ? "bg-[#2563eb] text-white" : "bg-black/5 text-[#616161]"
          }`}
        >
          {String(active)}
        </span>
      </div>
    </div>
  );
}
