import SectionShell from "../layout/SectionShell";

export default function HitTest() {
  return (
    <SectionShell
      number="26"
      title="Hit Test"
      description="Control pointer event handling with HitTest::Opaque and HitTest::Passthrough. Opaque captures events, Passthrough lets them through."
      code={`view! {\n    <Group\n        width={TRACK_W}\n        height={TRACK_H}\n        hit_test={HitTest::Opaque}\n        on:drag-start={move |info| { /* ... */ }}\n        on:drag-step={move |info| { /* ... */ }}\n    >\n        <Group\n            hit_test={HitTest::Passthrough}\n            // Events pass through to parent\n        />\n    </Group>\n}`}
    >
      <div className="flex flex-col items-center gap-1">
        <div className="w-16 h-4 bg-[#D8D5D0] rounded relative">
          <div className="absolute top-0.5 left-2 w-3 h-3 bg-[#483050] rounded-full" />
        </div>
        <span className="text-[7px] text-gray-500">drag the thumb</span>
      </div>
    </SectionShell>
  );
}
