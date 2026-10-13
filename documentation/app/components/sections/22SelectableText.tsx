import SectionShell from "../layout/SectionShell";

export default function SelectableText() {
  return (
    <SectionShell
      number="22"
      title="Selectable Text"
      description="Native text selection with selectable={true}. Press-drag highlights, double-click selects word, triple-click selects paragraph."
      code={`view! {\n    <Group\n        direction="vertical"\n        width="fill"\n        fill={Color::WHITE}\n        alignment="center"\n        distribution="center"\n    >\n        <Text\n            selectable={true}\n            size={17.0}\n            line_height={26.0}\n            color={Color::BLACK}\n            width={Size::fixed(520.0)}\n        >{LOREM}</Text>\n    </Group>\n}`}
    >
      <div className="flex flex-col items-center gap-1">
        <div className="text-[8px] text-black bg-blue-200 px-1 rounded">
          Drag to select
        </div>
        <div className="text-[7px] text-gray-500">
          Double-click: word · Triple-click: paragraph
        </div>
      </div>
    </SectionShell>
  );
}
