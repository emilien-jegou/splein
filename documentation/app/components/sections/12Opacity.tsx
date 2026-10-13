import SectionShell from "../layout/SectionShell";

export default function Opacity() {
  return (
    <SectionShell
      number="12"
      title="Opacity"
      description="Compositor opacity override. Animates without re-running layout — paint-only invalidation."
      code={`let opacity = Transition::from_str("out-cubic 180ms")\n    .on(is_open.map(move |open| {\n        if open { 1.0 } else { 0.0 }\n    }));\n\nview! {\n    <Group\n        direction="vertical"\n        width="fill"\n        height="fill"\n        opacity={opacity}\n    >\n        <Text size={13.0} color={BODY}>{copy}</Text>\n    </Group>\n}`}
    >
      <div className="flex flex-col items-center gap-2">
        <div className="w-16 h-8 bg-[#1019EC] rounded-lg flex items-center justify-center opacity-100">
          <span className="text-white text-[8px]">1.0</span>
        </div>
        <div className="w-16 h-8 bg-[#1019EC] rounded-lg flex items-center justify-center opacity-40">
          <span className="text-white text-[8px]">0.4</span>
        </div>
        <div className="w-16 h-8 bg-[#1019EC] rounded-lg flex items-center justify-center opacity-0">
          <span className="text-white text-[8px]">0.0</span>
        </div>
      </div>
    </SectionShell>
  );
}
