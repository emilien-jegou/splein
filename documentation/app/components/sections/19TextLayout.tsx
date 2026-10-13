import SectionShell from "../layout/SectionShell";

export default function TextLayout() {
  return (
    <SectionShell
      number="19"
      title="Text Layout"
      description="text_align, letter_spacing, weight, italic, underline, strikethrough, overline, max_lines, line_height_multiple."
      code={`view! {\n    <Group direction="vertical" width="fill" gap={14.0}>\n        <Text size={34.0} weight={700u16}\n            line_height_multiple={1.1}\n            color={Color::WHITE}>"Typography"</Text>\n        <Text size={15.0} weight={300}>"Light weight 300"</Text>\n        <Text size={15.0} weight={700}>"Bold weight 700"</Text>\n        <Text size={15.0} italic()>"Italic face"</Text>\n        <Text size={15.0} underline()>"Underlined"</Text>\n        <Text size={15.0} strikethrough()>"Struck"</Text>\n        <Text size={15.0} overline()>"Overlined"</Text>\n        <Text size={15.0} max_lines(2)>{ELLIPSIS}</Text>\n        <Text size={15.0} letter_spacing(3.0)>"Wide tracking"</Text>\n        <Text text_align(TextAlign::Justify)>{JUSTIFY}</Text>\n    </Group>\n}`}
    >
      <div className="flex flex-col items-start gap-0.5">
        <span className="text-[10px] font-bold text-black">Typography</span>
        <span className="text-[7px] font-light text-black">Light 300</span>
        <span className="text-[7px] font-bold text-black">Bold 700</span>
        <span className="text-[7px] italic text-black">Italic</span>
        <span className="text-[7px] underline text-black">Underline</span>
        <span className="text-[7px] line-through text-black">Strike</span>
      </div>
    </SectionShell>
  );
}
