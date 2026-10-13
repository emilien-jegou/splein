import SectionShell from "../layout/SectionShell";

export default function Overlay() {
  return (
    <SectionShell
      number="24"
      title="Overlay & Z-Index"
      description="Overlay positioning with overlay={true} and z_index for stacking. Elements paint above normal flow."
      code={`view! {\n    <Group\n        width={LOGO_W}\n        height={LOGO_H}\n        overlay={true}\n        z_index={100}\n        anchor={Anchor::TopLeft}\n        fill={color_sig}\n        transform={move || {\n            let (x, y) = pos.get();\n            Transform::from_translation(x, y)\n        }}\n    >\n        {svg(logo_graphic)}\n    </Group>\n}`}
    >
      <div className="relative w-20 h-16 bg-gray-100 rounded border border-gray-300">
        <div className="absolute top-1 left-1 w-8 h-4 bg-pink-400 rounded opacity-80 z-10" />
        <div className="absolute top-6 left-6 w-8 h-4 bg-blue-400 rounded opacity-80" />
      </div>
    </SectionShell>
  );
}
