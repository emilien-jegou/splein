import SectionShell from "../layout/SectionShell";

export default function LightEnvironment() {
  return (
    <SectionShell
      number="27"
      title="Light Environment"
      description="Physical lighting with LightEnvironment::from_angles. Controls shadow direction and falloff for elevation shadows."
      code={`view! {\n    <Group\n        light={LightEnvironment::from_angles(270.0, 45.0)}\n        width={110.0}\n        height={110.0}\n        radius={8.0}\n        fill={Color::WHITE}\n        shadow={12.0}\n    >\n        <Text size={12.0} color={0x483050}>"light az 270"</Text>\n    </Group>\n}`}
    >
      <div className="flex flex-col items-center gap-1">
        <div className="w-10 h-10 bg-white rounded-lg shadow-[4px_4px_8px_rgba(0,0,0,0.2)]" />
        <span className="text-[7px] text-gray-500">az 270° alt 45°</span>
      </div>
    </SectionShell>
  );
}
