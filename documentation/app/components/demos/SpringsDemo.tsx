"use client";

import { useState } from "react";
import { motion } from "motion/react";

const PRESETS = [
  { name: "default", stiffness: 300, damping: 30 },
  { name: "bouncy", stiffness: 400, damping: 12 },
  { name: "smooth", stiffness: 120, damping: 24 },
] as const;

export default function SpringsDemo() {
  const [preset, setPreset] = useState(0);
  const [target, setTarget] = useState(0);
  const p = PRESETS[preset];

  return (
    <div className="flex flex-col items-center gap-5">
      <div className="relative flex h-24 w-full max-w-112 items-center">
        <div className="absolute inset-x-2 h-px bg-gray-300" />
        <motion.div
          animate={{ x: target }}
          transition={{ type: "spring", stiffness: p.stiffness, damping: p.damping }}
          className="relative h-12 w-12 rounded-full bg-[#CA6D60]"
        />
      </div>
      <div className="flex items-center gap-2">
        <button
          onClick={() => setTarget(-80)}
          className="cursor-pointer rounded bg-[#2B3A4D] px-3 py-1.5 text-xs font-semibold text-white select-none"
        >
          left
        </button>
        <button
          onClick={() => setTarget(80)}
          className="cursor-pointer rounded bg-[#2B3A4D] px-3 py-1.5 text-xs font-semibold text-white select-none"
        >
          right
        </button>
      </div>
      <div className="flex gap-1">
        {PRESETS.map((pr, i) => (
          <button
            key={pr.name}
            onClick={() => setPreset(i)}
            className={`cursor-pointer rounded px-2.5 py-1 text-[10px] select-none ${
              i === preset
                ? "bg-[#2563eb] text-white"
                : "bg-black/5 text-[#616161]"
            }`}
          >
            {pr.name}
          </button>
        ))}
      </div>
    </div>
  );
}
