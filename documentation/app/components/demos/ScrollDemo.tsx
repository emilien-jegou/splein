"use client";

import { useEffect, useRef } from "react";

const ROWS = 40;

export default function ScrollDemo() {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    let raf = 0;
    const tick = () => {
      if (el.scrollTop + el.clientHeight >= el.scrollHeight - 1) {
        el.scrollTop = 0;
      } else {
        el.scrollTop += 1;
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, []);

  return (
    <div
      ref={ref}
      className="h-48 w-full max-w-96 overflow-y-auto rounded border border-gray-300 bg-white"
    >
      {Array.from({ length: ROWS }, (_, i) => (
        <div
          key={i}
          className={`flex h-8 items-center border-b border-gray-200 px-3 text-xs ${
            i % 2 ? "bg-[#F5F5F7]" : "bg-[#EDEFF3]"
          }`}
        >
          Row {String(i).padStart(2, "0")}
        </div>
      ))}
    </div>
  );
}
