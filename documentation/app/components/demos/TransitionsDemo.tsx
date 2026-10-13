"use client";

import { useState } from "react";
import { motion } from "motion/react";

export default function TransitionsDemo() {
  const [on, setOn] = useState(true);

  return (
    <div className="flex flex-col items-center gap-6">
      <button
        onClick={() => setOn((o) => !o)}
        className="cursor-pointer rounded bg-[#2B3A4D] px-4 py-2 text-sm font-semibold text-white select-none"
      >
        toggle
      </button>
      <div className="flex items-end gap-8">
        <div className="flex flex-col items-center gap-2">
          <motion.div
            animate={{ opacity: on ? 1 : 0.35, x: on ? 0 : 24 }}
            transition={{ type: "tween", duration: 0.2, ease: "easeOut" }}
            className="h-12 w-12 rounded bg-[#2563eb]"
          />
          <span className="text-[10px] text-[#616161]">tween</span>
        </div>
        <div className="flex flex-col items-center gap-2">
          <motion.div
            animate={{ scale: on ? 1.18 : 1 }}
            transition={{ type: "spring", stiffness: 300, damping: 15 }}
            className="h-12 w-12 rounded-full bg-[#CA6D60]"
          />
          <span className="text-[10px] text-[#616161]">spring</span>
        </div>
        <div className="flex flex-col items-center gap-2">
          <motion.div
            animate={{ rotate: on ? 0 : 180 }}
            transition={{ type: "spring", stiffness: 200, damping: 20 }}
            className="h-12 w-12 rounded-md bg-[#336133]"
          />
          <span className="text-[10px] text-[#616161]">rotate</span>
        </div>
      </div>
    </div>
  );
}
