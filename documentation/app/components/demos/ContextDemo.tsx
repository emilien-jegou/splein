"use client";

import { createContext, useContext, useState } from "react";

const StatusContext = createContext<{
  status: string;
  setStatus: (s: string) => void;
}>({ status: "idle", setStatus: () => {} });

function Provider({ children }: { children: React.ReactNode }) {
  const [status, setStatus] = useState("idle");
  return (
    <StatusContext.Provider value={{ status, setStatus }}>
      {children}
    </StatusContext.Provider>
  );
}

function Consumer() {
  const { status, setStatus } = useContext(StatusContext);
  return (
    <div className="flex flex-col items-center gap-3">
      <div className="flex gap-2">
        {["idle", "loading", "done"].map((s) => (
          <button
            key={s}
            onClick={() => setStatus(s)}
            className={`cursor-pointer rounded px-3 py-1.5 text-xs font-semibold select-none ${
              status === s
                ? "bg-[#2563eb] text-white"
                : "bg-black/5 text-[#616161]"
            }`}
          >
            {s}
          </button>
        ))}
      </div>
      <span className="font-mono text-xs text-[#616161]">
        context status: <span className="font-semibold text-black">{status}</span>
      </span>
    </div>
  );
}

export default function ContextDemo() {
  return (
    <Provider>
      <Consumer />
    </Provider>
  );
}
