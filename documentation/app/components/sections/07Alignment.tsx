import SectionShell from "../layout/SectionShell";

export default function Alignment() {
  return (
    <SectionShell
      number="07"
      title="Alignment & Distribution"
      description="Control child positioning with alignment (start/center/end) and distribution (start/center/end/space-between)."
      code={`view! {\n    <Group\n        direction="horizontal"\n        width="fill"\n        height="fill"\n        alignment="center"\n        distribution="space-between"\n    >\n        <Text size={14.0}>"Left"</Text>\n        <Text size={14.0}>"Center"</Text>\n        <Text size={14.0}>"Right"</Text>\n    </Group>\n}`}
    >
      <div className="flex flex-row items-center justify-between w-full px-4">
        <span className="text-[10px] text-black">Left</span>
        <span className="text-[10px] text-black">Center</span>
        <span className="text-[10px] text-black">Right</span>
      </div>
    </SectionShell>
  );
}
