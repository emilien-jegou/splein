import SectionShell from "../layout/SectionShell";

export default function Anchors() {
  return (
    <SectionShell
      number="06"
      title="Anchors"
      description="Position elements within their parent using Anchor::TopRight, Anchor::BottomRight, Anchor::Center, etc."
      code={`view! {\n    <Group width={116.0} height={45.0} fill={0xFFFFFF}>\n        <Text\n            size={12.0}\n            color={Color::BLACK}\n            anchor={Anchor::BottomRight}\n        >"bottom-right"</Text>\n    </Group>\n}`}
    >
      <div className="relative w-[58px] h-[22px] bg-white border border-gray-300">
        <span className="absolute bottom-0 right-0 text-[7px] text-black">br</span>
      </div>
    </SectionShell>
  );
}
