import SectionShell from "../layout/SectionShell";

export default function Scroll() {
  return (
    <SectionShell
      number="08"
      title="Scroll"
      description="ScrollView and VirtualScrollView with use_scroll, scroll_offset, and scroll_by. Supports 100k+ row virtualization."
      code={`let sc = use_scroll(0.0);\n\nview! {\n    <ScrollView\n        width={Size::fill()}\n        height={Size::fill()}\n        handle={sc}\n    >\n        {scroll_content}\n    </ScrollView>\n}\n\n// Virtualized: only visible rows mounted\n<VirtualScrollView\n    len={100_000usize}\n    row_height={36.0}\n    handle={handle}\n    row={vrow}\n/>`}
    >
      <div className="w-32 h-16 bg-white rounded border border-gray-300 overflow-hidden">
        <div className="h-4 bg-[#EDEFF3] border-b border-gray-200 flex items-center px-1">
          <span className="text-[6px] text-gray-600">row 00</span>
        </div>
        <div className="h-4 bg-[#E2E5EC] border-b border-gray-200 flex items-center px-1">
          <span className="text-[6px] text-gray-600">row 01</span>
        </div>
        <div className="h-4 bg-[#EDEFF3] border-b border-gray-200 flex items-center px-1">
          <span className="text-[6px] text-gray-600">row 02</span>
        </div>
      </div>
    </SectionShell>
  );
}
