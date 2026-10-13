import SectionShell from "../layout/SectionShell";

export default function FontFamily() {
  return (
    <SectionShell
      number="35"
      title="Font Family"
      description="Custom font loading with font_family. Register fonts at startup, reference by name in any Text component."
      code={`// Register at startup\ncli.app_config("Aurora")\n    .font("Inter", include_bytes!("../assets/inter-var.ttf"))\n    .run(move || build_ui());\n\n// Use in any Text\nview! {\n    <Text\n        font_family="Inter"\n        size={15.0}\n        color={0xE2E8F0}\n    >"Custom font"</Text>\n}`}
    >
      <div className="flex flex-col items-center gap-1">
        <span className="text-[10px] text-black" style={{ fontFamily: 'Inter, system-ui, sans-serif' }}>
          Inter
        </span>
        <span className="text-[8px] text-black" style={{ fontFamily: 'system-ui, sans-serif' }}>
          system-ui
        </span>
      </div>
    </SectionShell>
  );
}
