import SectionShell from "../layout/SectionShell";

export default function ElementRef() {
  return (
    <SectionShell
      number="32"
      title="Element Ref"
      description="use_ref and ElementRef for reading layout geometry. child_rects() measures all children for gesture targeting."
      code={`let row_ref = use_ref();\n\nview! {\n    <Group\n        ref={row_ref}\n        direction="horizontal"\n        width="fill"\n    >\n        <Text size={14.0}>{label}</Text>\n    </Group>\n}\n\n// Read child boxes\nlet slots: Vec<ResolvedRect> = list.child_rects().to_vec();`}
    >
      <div className="flex flex-col items-center gap-1">
        <div className="w-20 h-5 bg-gray-200 rounded border border-dashed border-gray-400 flex items-center justify-center">
          <span className="text-[6px] text-gray-500">ref</span>
        </div>
        <span className="text-[6px] text-gray-400">child_rects()</span>
      </div>
    </SectionShell>
  );
}
