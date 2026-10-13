"use client";

import { useState } from "react";
import { motion } from "motion/react";

const TRACK = 200;
const THUMB = 18;

export default function DynamicPropDemo() {
  const [value, setValue] = useState(0.3);
  const frac = Math.min(1, Math.max(0, value));
  const x = frac * (TRACK - THUMB);

  return (
    <div className="flex w-full max-w-96 flex-col items-center gap-5">
      <div className="relative" style={{ width: TRACK }}>
        <div className="h-2 rounded-full bg-gray-300" />
        <motion.div
          animate={{ x }}
          transition={{ type: "spring", stiffness: 400, damping: 32 }}
          className="absolute top-1/2 -mt-2.5 h-5 w-5 rounded-full bg-[#483050] shadow"
          style={{ marginLeft: 0 }}
        />
      </div>
      <input
        type="range"
        min={0}
        max={1}
        step={0.01}
        value={value}
        onChange={(e) => setValue(Number(e.target.value))}
        className="w-56"
      />
      <span className="font-mono text-xs tabular-nums text-[#616161]">
        thumb_x = frac × (track − thumb) = {Math.round(x)}px
      </span>
    </div>
  );
}
