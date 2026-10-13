"use client";

import { useState } from "react";
import { motion, useAnimationFrame, useMotionValue } from "motion/react";

export default function PlaybackDemo() {
  const [paused, setPaused] = useState(false);
  const x = useMotionValue(0);

  useAnimationFrame((time) => {
    if (!paused) {
      x.set(Math.sin(time / 500) * 90);
    }
  });

  return (
    <div className="flex flex-col items-center gap-3">
      <div className="relative flex h-16 w-full max-w-112 items-center justify-center rounded-lg bg-white/60">
        <div className="absolute inset-x-4 h-px bg-gray-300" />
        <motion.div
          style={{ x }}
          onHoverStart={() => setPaused(true)}
          onHoverEnd={() => setPaused(false)}
          className="relative flex h-10 items-center rounded bg-[#2B3A4D] px-4 text-sm text-white"
        >
          pause on hover
        </motion.div>
      </div>
      <span className="text-[10px] text-[#616161]">
        {paused ? "paused" : "playing · ping-pong"}
      </span>
    </div>
  );
}
