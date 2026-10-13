"use client";

import { useState } from "react";

export default function LightEnvironmentDemo() {
  const [az, setAz] = useState(270);
  const rad = (az * Math.PI) / 180;
  const sx = Math.round(Math.cos(rad) * 10);
  const sy = Math.round(Math.sin(rad) * 10);

  return (
    <div className="flex flex-col items-center gap-5">
      <div
        className="flex h-20 w-20 items-center justify-center rounded-xl bg-white"
        style={{ boxShadow: `${sx}px ${sy}px 14px rgba(0,0,0,0.28)` }}
      >
        <span className="text-[10px] text-[#616161]">az {az}°</span>
      </div>
      <input
        type="range"
        min={0}
        max={360}
        value={az}
        onChange={(e) => setAz(Number(e.target.value))}
        className="w-48"
      />
      <span className="font-mono text-[10px] text-[#616161]">
        shadow offset {sx}, {sy}
      </span>
    </div>
  );
}
