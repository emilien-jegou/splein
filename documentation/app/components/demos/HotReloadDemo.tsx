"use client";

import { useState } from "react";
import { AnimatePresence, motion } from "motion/react";

export default function HotReloadDemo() {
  const [key, setKey] = useState(0);
  const [seed, setSeed] = useState(1);

  const reload = () => {
    setKey((k) => k + 1);
    setSeed(Math.random());
  };

  return (
    <div className="flex flex-col items-center gap-4">
      <div className="flex items-center gap-3">
        <button
          onClick={reload}
          className="cursor-pointer rounded bg-[#2563eb] px-4 py-2 text-sm font-semibold text-white select-none"
        >
          reload
        </button>
        <span className="font-mono text-xs text-[#616161]">
          rev {key}
        </span>
      </div>
      <AnimatePresence mode="wait">
        <motion.div
          key={key}
          initial={{ opacity: 0, scale: 0.96 }}
          animate={{ opacity: 1, scale: 1 }}
          exit={{ opacity: 0, scale: 0.96 }}
          transition={{ duration: 0.15 }}
          className="flex h-20 w-48 flex-col items-center justify-center rounded-lg bg-[#2B3A4D]"
        >
          <span className="text-sm text-white">UI v{seed.toFixed(2)}</span>
          <span className="text-[9px] text-white/60">hot-reloaded</span>
        </motion.div>
      </AnimatePresence>
    </div>
  );
}
