"use client";

import { useState } from "react";
import { Reorder } from "motion/react";

const INITIAL = ["Alpha", "Beta", "Gamma", "Delta", "Epsilon"];

export default function KeyedForDemo() {
  const [items, setItems] = useState(INITIAL);

  const shuffle = () =>
    setItems((arr) => [...arr].sort(() => Math.random() - 0.5));

  return (
    <div className="flex flex-col items-center gap-4">
      <Reorder.Group
        axis="y"
        values={items}
        onReorder={setItems}
        className="flex w-full max-w-72 flex-col gap-2"
      >
        {items.map((item) => (
          <Reorder.Item
            key={item}
            value={item}
            className="flex cursor-grab items-center justify-between rounded bg-[#336133] px-4 py-2 text-sm text-white active:cursor-grabbing"
          >
            <span>{item}</span>
            <span className="opacity-50">⠿</span>
          </Reorder.Item>
        ))}
      </Reorder.Group>
      <button
        onClick={shuffle}
        className="cursor-pointer rounded bg-[#2B3A4D] px-4 py-1.5 text-xs font-semibold text-white select-none"
      >
        shuffle
      </button>
    </div>
  );
}
