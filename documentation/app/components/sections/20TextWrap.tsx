import SectionShell from "../layout/SectionShell";

export default function TextWrap() {
  return (
    <SectionShell
      number="20"
      title="Text Wrap"
      description="text_wrap with wrap/nowrap/balance modes. flow_wrap for flex-like wrapping. aspect_ratio for proportional sizing."
      code={`view! {\n    <Group direction="vertical" width="fill" gap={12.0}>\n        // Wrap (default)\n        {text(copy).size(15.0).color("#FFFFFF")}\n\n        // NoWrap: single line, clipped\n        {text(copy).size(15.0).text_wrap("nowrap")}\n\n        // Balance: even line lengths\n        {text(copy).size(22.0).text_wrap("balance")}\n\n        // FlowWrap: flex-like wrapping\n        <Group width={340.0} flow_wrap="wrap" gap={8.0}>\n            <Chip label="Submit" />\n            <Chip label="Cancel the whole thing" />\n        </Group>\n\n        // Aspect ratio\n        <Group width={320.0} aspect_ratio="16/9" fill="#24405E" />\n    </Group>\n}`}
    >
      <div className="flex flex-col items-center gap-2">
        <div className="w-32 text-[7px] text-black leading-tight">
          Pack my box with five dozen liquor jugs
        </div>
        <div className="w-32 text-[7px] text-black leading-tight whitespace-nowrap overflow-hidden text-ellipsis">
          Pack my box with five dozen liquor jugs
        </div>
        <div className="flex flex-wrap gap-1 w-32">
          <span className="px-1 py-0.5 bg-[#2B3A4D] rounded text-[6px] text-white">Submit</span>
          <span className="px-1 py-0.5 bg-[#2B3A4D] rounded text-[6px] text-white">Cancel</span>
          <span className="px-1 py-0.5 bg-[#2B3A4D] rounded text-[6px] text-white">OK</span>
        </div>
      </div>
    </SectionShell>
  );
}
