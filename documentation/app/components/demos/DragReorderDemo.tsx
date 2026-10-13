"use client";

import { useState } from "react";
import { Reorder } from "motion/react";

const INITIAL = ["Baseline", "Underline", "Strikethrough", "Overline", "Italic"];

export default function DragReorderDemo() {
  const [rows, setRows] = useState(INITIAL);

  return (
    <Reorder.Group
      axis="y"
      values={rows}
      onReorder={setRows}
      className="flex w-full max-w-80 flex-col gap-2"
    >
      {rows.map((r, i) => (
        <Reorder.Item
          key={r}
          value={r}
          className="flex cursor-grab items-center justify-between rounded bg-[#1019EC] px-4 py-2.5 text-sm text-white active:cursor-grabbing"
        >
          <span>
            {i + 1} {r}
          </span>
          <span className="opacity-50">⠿</span>
        </Reorder.Item>
      ))}
    </Reorder.Group>
  );
}
