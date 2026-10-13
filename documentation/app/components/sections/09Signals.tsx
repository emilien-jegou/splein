import SectionShell from "../layout/SectionShell";

export default function Signals() {
  return (
    <SectionShell
      number="09"
      title="Signals"
      description="Reactive state with use_signal, Derived, derived!, and .map(). Automatic dependency tracking drives re-renders."
      code={`let count = use_signal(0);\nlet label = count.map(|c| format!("Count: {c}"));\n\n// derived! with automatic dependency tracking\nlet active = derived!(move || {\n    if pressed.get() { down.get() } else { up.get() }\n});\n\nview! {\n    <Group on:click={|_| count.update(|c| *c += 1)}>\n        <Text size={14.0}>{label}</Text>\n    </Group>\n}`}
    >
      <div className="flex flex-col items-center gap-1">
        <div className="px-3 py-1 bg-[#2563eb] rounded">
          <span className="text-white text-[10px]">Count: 0</span>
        </div>
        <span className="text-[8px] text-gray-400">click to +1</span>
      </div>
    </SectionShell>
  );
}
