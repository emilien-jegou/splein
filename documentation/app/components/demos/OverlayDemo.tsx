"use client";

import {
  motion,
  useAnimationFrame,
  useMotionValue,
  useTransform,
} from "motion/react";

export default function OverlayDemo() {
  const t = useMotionValue(0);
  useAnimationFrame((time) => {
    t.set(time / 1000);
  });
  const x = useTransform(t, (v) => Math.sin(v) * 80);
  const y = useTransform(t, (v) => Math.cos(v * 0.7) * 24);

  return (
    <div className="relative h-32 w-full max-w-112 overflow-hidden rounded-lg bg-[#24405E]">
      <div className="absolute inset-x-0 bottom-0 h-10 bg-black/25" />
      <div className="absolute top-3 left-3 text-xs text-white/70">
        base layer
      </div>
      <motion.div
        style={{ x, y }}
        className="absolute top-1/2 left-1/2 -ml-7 -mt-7 z-10 flex h-14 w-14 items-center justify-center rounded-lg bg-[#FF00FF] shadow-xl"
      >
        <span className="text-[9px] font-semibold text-white">overlay</span>
      </motion.div>
    </div>
  );
}
