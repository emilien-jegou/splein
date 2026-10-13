import SectionShell from "../layout/SectionShell";

export default function FillStrokeRadius() {
  return (
    <SectionShell
      number="05"
      title="Fill, Stroke & Radius"
      description="Visual styling: background fill, border stroke (inside/outside), corner radius, and clipping."
      code={`view! {\n    <Group\n        direction="vertical"\n        width={140.0}\n        height={144.0}\n        radius="max"\n        fill={0xFF00FF}\n        stroke={Stroke::inside(3.0, Color::BLACK)}\n        clip={true}\n        alignment="center"\n        distribution="center"\n    >\n        <Text size={12.0} color={Color::WHITE}>"with radius"</Text>\n    </Group>\n}`}
    >
      <div className="w-[70px] h-[72px] rounded-full bg-[#FF00FF] border-[3px] border-black flex items-center justify-center">
        <span className="text-white text-[8px]">radius</span>
      </div>
    </SectionShell>
  );
}
