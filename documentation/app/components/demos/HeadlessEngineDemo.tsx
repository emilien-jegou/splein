"use client";

import { useEffect, useRef, useState } from "react";

export default function HeadlessEngineDemo() {
  const [fps, setFps] = useState(0);
  const [hist, setHist] = useState<number[]>(() => Array(20).fill(16.7));
  const last = useRef(performance.now());
  const acc = useRef(0);
  const n = useRef(0);

  useEffect(() => {
    let raf = 0;
    const tick = (now: number) => {
      acc.current += now - last.current;
      last.current = now;
      n.current++;
      if (acc.current >= 500) {
        const avg = acc.current / n.current;
        setFps(Math.round(1000 / avg));
        setHist((h) => [...h.slice(-19), Math.round(avg * 10) / 10]);
        acc.current = 0;
        n.current = 0;
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, []);

  return (
    <div className="flex w-full max-w-96 flex-col items-center gap-3">
      <div className="flex items-end gap-3">
        <span className="font-mono text-4xl font-semibold tabular-nums text-black">
          {fps}
        </span>
        <span className="pb-1 text-xs text-[#616161]">fps</span>
      </div>
      <div className="flex h-12 items-end gap-1">
        {hist.map((v, i) => (
          <div
            key={i}
            className="w-2.5 rounded-sm bg-[#2563eb]/80"
            style={{ height: `${Math.min(100, (v / 33) * 100)}%` }}
          />
        ))}
      </div>
      <span className="text-[10px] text-[#616161]">
        frame time benchmark · draw_engine
      </span>
    </div>
  );
}
