"use client";

import { useState } from "react";
import { motion } from "motion/react";

export default function PressFeedbackDemo() {
  const [count, setCount] = useState(0);

  return (
    <div className="flex flex-col items-center gap-4">
      <motion.button
        whileTap={{ scale: 0.9 }}
        transition={{ type: "spring", stiffness: 500, damping: 20 }}
        onClick={() => setCount((c) => c + 1)}
        className="cursor-pointer rounded-full bg-[#CA6D60] px-6 py-3 text-sm font-semibold text-white select-none"
      >
        Continue
      </motion.button>
      <span className="font-mono text-xs text-[#616161]">
        clicks: {count}
      </span>
    </div>
  );
}
