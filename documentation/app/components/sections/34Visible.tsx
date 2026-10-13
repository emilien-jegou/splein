import SectionShell from "../layout/SectionShell";

export default function Visible() {
  return (
    <SectionShell
      number="34"
      title="Visible"
      description="The visible prop controls rendering. When false, the element is skipped entirely — no layout, no paint."
      code={`view! {\n    <Group direction="vertical" gap={12.0}>\n        <Group visible={in_web.clone()}>\n            {slider("offset X", ox.clone(), -40.0, 40.0)}\n            {slider("offset Y", oy.clone(), -40.0, 40.0)}\n        </Group>\n        <Group visible={in_elev.clone()}>\n            {slider("elevation", elevation.clone(), 0.0, 32.0)}\n        </Group>\n    </Group>\n}`}
    >
      <div className="flex flex-col items-center gap-1">
        <div className="px-2 py-0.5 bg-green-100 rounded">
          <span className="text-[7px] text-green-700">visible: true</span>
        </div>
        <div className="px-2 py-0.5 bg-gray-100 rounded opacity-30">
          <span className="text-[7px] text-gray-500">visible: false</span>
        </div>
      </div>
    </SectionShell>
  );
}
