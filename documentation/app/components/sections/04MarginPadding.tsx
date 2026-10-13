import SectionShell from "../layout/SectionShell";

export default function MarginPadding() {
  return (
    <SectionShell
      number="04"
      title="Margin & Padding"
      description="Spacing control with Margin::y, Margin::x, Margin::sides, and padding. Gap between children."
      code={`view! {\n    <Group\n        direction="vertical"\n        width="fill"\n        margin={Margin::y(41.0)}\n        padding={12.0}\n        gap={16.0}\n    >\n        <Group\n            margin={Margin::sides(0.0, 20.0, 20.0, 20.0)}\n            padding="8 14"\n        >\n            <Text size={14.0}>"Spaced"</Text>\n        </Group>\n    </Group>\n}`}
    >
      <div className="flex flex-col items-center gap-2 p-4">
        <div className="px-5 py-2 bg-[#2B3A4D] rounded-full">
          <span className="text-white text-xs">padding 8 14</span>
        </div>
        <div className="px-3 py-1 bg-[#1B2430] rounded">
          <span className="text-white text-[10px]">gap: 16</span>
        </div>
      </div>
    </SectionShell>
  );
}
