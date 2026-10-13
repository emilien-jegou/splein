import SectionShell from "../layout/SectionShell";

export default function VectorGraphic() {
  return (
    <SectionShell
      number="23"
      title="Vector Graphic"
      description="SVG rendering with VectorGraphic::from_str. Anti-aliased vector graphics via TinySkia or Vello."
      code={`let logo_graphic = VectorGraphic::from_str(DVD_SVG_RAW)\n    .expect("Valid SVG");\n\nview! {\n    <Group\n        width={LOGO_W}\n        height={LOGO_H}\n        overlay={true}\n        z_index={100}\n        fill={color_sig}\n    >\n        {svg(logo_graphic)}\n    </Group>\n}`}
    >
      <div className="flex flex-col items-center gap-1">
        <svg width="40" height="20" viewBox="0 0 40 20">
          <rect width="40" height="20" rx="3" fill="#EC4899" opacity="0.8" />
          <text x="20" y="13" textAnchor="middle" fontSize="7" fill="white">SVG</text>
        </svg>
      </div>
    </SectionShell>
  );
}
