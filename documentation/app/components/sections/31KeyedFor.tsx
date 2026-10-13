import SectionShell from "../layout/SectionShell";

export default function KeyedFor() {
  return (
    <SectionShell
      number="31"
      title="Keyed For"
      description="The <for> block with key= for keyed reconciliation. Items are matched by key across re-renders, preserving component state."
      code={`view! {\n    <Group\n        direction="vertical"\n        width="fill"\n        height="fill"\n        gap={GAP}\n        ref={list_ref}\n    >\n        <for (i, spec) in items key={.label.clone()}>\n            <Row index={i} spec={spec} />\n        </for>\n    </Group>\n}`}
    >
      <div className="flex flex-col items-center gap-1">
        <div className="w-20 h-5 bg-[#1019EC] rounded flex items-center justify-center">
          <span className="text-white text-[6px]">key: label</span>
        </div>
        <span className="text-[6px] text-gray-400">state preserved</span>
      </div>
    </SectionShell>
  );
}
