import SectionShell from "../layout/SectionShell";

export default function AutoGrid() {
  return (
    <SectionShell
      number="21"
      title="Auto Grid"
      description="Client-side auto grid with auto_grid::columns_for. Dynamically computes columns and child widths."
      code={`let width = use_signal(420.0f32);\nlet (cols, child_w) = aurora::dsl::auto_grid::columns_for(\n    width.get(), min_child, gap\n);\n\nview! {\n    <Group direction="vertical" width="fill" gap={8.0}>\n        <AutoGrid width={width} min_child={150.0} gap={8.0} />\n    </Group>\n}`}
    >
      <div className="flex flex-col items-center gap-1">
        <div className="flex gap-1">
          <div className="w-8 h-6 bg-[#2B3A4D] rounded flex items-center justify-center">
            <span className="text-white text-[5px]">One</span>
          </div>
          <div className="w-8 h-6 bg-[#2B3A4D] rounded flex items-center justify-center">
            <span className="text-white text-[5px]">Two</span>
          </div>
          <div className="w-8 h-6 bg-[#2B3A4D] rounded flex items-center justify-center">
            <span className="text-white text-[5px]">Three</span>
          </div>
        </div>
        <div className="flex gap-1">
          <div className="w-8 h-6 bg-[#2B3A4D] rounded flex items-center justify-center">
            <span className="text-white text-[5px]">Four</span>
          </div>
          <div className="w-8 h-6 bg-[#2B3A4D] rounded flex items-center justify-center">
            <span className="text-white text-[5px]">Five</span>
          </div>
        </div>
      </div>
    </SectionShell>
  );
}
