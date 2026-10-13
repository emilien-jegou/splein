"use client";

import { useState } from "react";

export default function ViewMacroDemo() {
  const [count, setCount] = useState(0);

  return (
    <button
      onClick={() => setCount((c) => c + 1)}
      className="flex cursor-pointer items-center justify-center rounded bg-[#2563eb] px-5 py-3 select-none"
    >
      <span className="text-sm font-semibold text-white">Count: {count}</span>
    </button>
  );
}
