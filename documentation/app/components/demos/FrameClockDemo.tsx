"use client";

import {
  motion,
  useAnimationFrame,
  useMotionValue,
  useTransform,
} from "motion/react";

export default function FrameClockDemo() {
  const t = useMotionValue(0);
  useAnimationFrame((time) => {
    t.set(time / 1000);
  });
  const x = useTransform(t, (v) => Math.sin(v * 2) * 90);

  return (
    <div className="relative flex h-28 w-full max-w-112 items-center justify-center">
      <div className="absolute inset-x-4 h-px bg-gray-300" />
      <motion.div
        style={{ x }}
        className="relative h-9 w-9 rounded-full bg-[#2563eb] shadow-md"
      />
    </div>
  );
}
