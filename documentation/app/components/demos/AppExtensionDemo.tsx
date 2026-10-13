"use client";

import { useEffect, useRef, useState } from "react";

export default function AppExtensionDemo() {
  const [frames, setFrames] = useState(0);
  const start = useRef(performance.now());

  useEffect(() => {
    let raf = 0;
    let n = 0;
    const tick = (now: number) => {
      n++;
      setFrames(Math.round((now - start.current) / 1000 * 10) / 10 * 0 + n);
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, []);

  return (
    <div className="flex flex-col items-center gap-2">
      <span className="font-mono text-4xl font-semibold tabular-nums text-black">
        {frames}
      </span>
      <span className="text-xs text-[#616161]">
        frames · on_frame ticker
      </span>
    </div>
  );
}
