import SectionShell from "../layout/SectionShell";

export default function DynamicProp() {
  return (
    <SectionShell
      number="33"
      title="Dynamic Prop"
      description="DynamicProp and computed() for reactive property values. Values update automatically when signals change."
      code={`let thumb_x = computed(move || {\n    let frac = ((value.get() - min) / (max - min)).clamp(0.0, 1.0);\n    frac * (TRACK_W - THUMB_W)\n});\n\nview! {\n    <Group\n        width={THUMB_W}\n        height={20.0}\n        radius={7.0}\n        fill={0x483050}\n        x={thumb_x}\n    />\n}`}
    >
      <div className="flex flex-col items-center gap-1">
        <div className="w-16 h-3 bg-[#D8D5D0] rounded-full relative">
          <div className="absolute top-0 left-4 w-3 h-3 bg-[#483050] rounded-full" />
        </div>
      </div>
    </SectionShell>
  );
}
