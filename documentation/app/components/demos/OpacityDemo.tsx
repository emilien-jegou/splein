"use client";

import { useState } from "react";
import { AnimatePresence, motion } from "motion/react";

export default function OpacityDemo() {
  const [open, setOpen] = useState(true);

  return (
    <div className="flex flex-col items-center gap-5">
      <button
        onClick={() => setOpen((o) => !o)}
        className="cursor-pointer rounded bg-[#2B3A4D] px-4 py-2 text-sm font-semibold text-white select-none"
      >
        toggle
      </button>
      <div className="flex h-20 w-full max-w-80 items-center justify-center rounded-lg bg-white/60">
        <AnimatePresence mode="wait">
          {open ? (
            <motion.div
              key="on"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.18, ease: "easeOut" }}
              className="rounded bg-[#2563eb] px-5 py-2.5 text-sm font-semibold text-white"
            >
              faded in
            </motion.div>
          ) : (
            <motion.div
              key="off"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.18 }}
              className="text-sm text-[#616161]"
            >
              faded out
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </div>
  );
}
