"use client";

import { useState } from "react";
import { AnimatePresence, motion } from "motion/react";

export default function VisibleDemo() {
  const [showA, setShowA] = useState(true);
  const [showB, setShowB] = useState(true);

  const Btn = ({
    on,
    label,
    onClick,
  }: {
    on: boolean;
    label: string;
    onClick: () => void;
  }) => (
    <button
      onClick={onClick}
      className={`cursor-pointer rounded px-3 py-1.5 text-xs font-semibold select-none ${
        on ? "bg-[#2563eb] text-white" : "bg-black/5 text-[#616161]"
      }`}
    >
      {label} {on ? "on" : "off"}
    </button>
  );

  return (
    <div className="flex flex-col items-center gap-4">
      <div className="flex gap-2">
        <Btn on={showA} label="offset" onClick={() => setShowA((s) => !s)} />
        <Btn on={showB} label="elev" onClick={() => setShowB((s) => !s)} />
      </div>
      <div className="flex h-14 items-center gap-3">
        <AnimatePresence>
          {showA && (
            <motion.div
              key="a"
              initial={{ opacity: 0, scale: 0.7 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.7 }}
              transition={{ type: "spring", stiffness: 400, damping: 25 }}
              className="h-12 w-20 rounded-lg bg-[#2B3A4D]"
            />
          )}
          {showB && (
            <motion.div
              key="b"
              initial={{ opacity: 0, scale: 0.7 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.7 }}
              transition={{ type: "spring", stiffness: 400, damping: 25 }}
              className="h-12 w-20 rounded-lg bg-[#CA6D60] shadow-xl"
            />
          )}
        </AnimatePresence>
        {!showA && !showB && (
          <span className="text-xs text-[#616161]">all hidden</span>
        )}
      </div>
    </div>
  );
}
