import SectionShell from "../layout/SectionShell";

export default function Group() {
  return (
    <SectionShell
      number="02"
      title="Group"
      description="The core layout container. Stacks children vertically or horizontally with direction, width, height, and fill."
      code={`view! {\n    <Group\n        direction="vertical"\n        width="fill"\n        height="fill"\n        fill={Color::WHITE}\n        alignment="center"\n        distribution="center"\n    >\n        <Group\n            direction="horizontal"\n            width={500.0}\n            height={204.0}\n            fill={0x336133}\n            gap={16.0}\n        >\n            <Text size={16.0} color={Color::WHITE}>"Hello"</Text>\n        </Group>\n    </Group>\n}`}
    >
      <div className="flex flex-col items-center gap-2">
        <div className="w-32 h-16 bg-[#336133] rounded flex items-center justify-center">
          <span className="text-white text-xs">500×204</span>
        </div>
        <div className="w-32 h-8 bg-[#CA6D60] rounded flex items-center justify-center">
          <span className="text-white text-[10px]">fill</span>
        </div>
      </div>
    </SectionShell>
  );
}
