"use client";

import {
  motion,
  useAnimationFrame,
  useMotionValue,
  useTransform,
} from "motion/react";

export default function TransformDemo() {
  const t = useMotionValue(0);
  useAnimationFrame((time) => {
    t.set(time / 1000);
  });
  const x = useTransform(t, (v) => Math.sin(v * 1.2) * 90);
  const rotate = useTransform(t, (v) => Math.sin(v * 1.2) * 12);

  return (
    <div className="relative flex h-28 w-full max-w-112 items-center justify-center">
      <div className="absolute inset-x-4 h-px bg-gray-300" />
      <motion.div
        style={{ x, rotate }}
        className="relative flex h-16 w-24 items-center justify-center rounded-lg bg-[#2B3A4D] shadow-lg"
      >
        <span className="text-sm text-white">card</span>
      </motion.div>
    </div>
  );
}
