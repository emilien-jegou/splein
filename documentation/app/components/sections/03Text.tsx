import SectionShell from "../layout/SectionShell";

export default function Text() {
  return (
    <SectionShell
      number="03"
      title="Text"
      description="Text rendering with size, weight, color, line height, and selectable. The primary content primitive."
      code={`view! {\n    <Group direction="vertical" width="fill" gap={8.0}>\n        <Text\n            selectable={true}\n            size={16.0}\n            line_height={20.0}\n            color={Color::WHITE}\n        >"Hello you"</Text>\n        <Text\n            size={12.0}\n            weight={700u16}\n            color={Color::BLACK}\n        >"Bold label"</Text>\n    </Group>\n}`}
    >
      <div className="flex flex-col items-start gap-1">
        <span className="text-[16px] text-black leading-[20px]">Hello you</span>
        <span className="text-[12px] font-bold text-black">Bold label</span>
      </div>
    </SectionShell>
  );
}
