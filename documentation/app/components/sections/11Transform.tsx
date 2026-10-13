import SectionShell from "../layout/SectionShell";

export default function Transform() {
  return (
    <SectionShell
      number="11"
      title="Transform"
      description="Compositor transforms: translation, rotation, scale. Runs per-frame without dirtying layout."
      code={`view! {\n    <Group\n        width={280.0}\n        height="fill"\n        x={travel}\n        transform={move || {\n            Transform::from_translation(x, y)\n        }}\n    >\n        <Text size={17.0} color={Color::WHITE}>"card"</Text>\n    </Group>\n}\n\n// Rotation\nTransform::from_rotation_degrees(angle)\n\n// Combined\nTransform::from_translation(v * 380.0, (v * 6.28).sin() * 9.0)`}
    >
      <div className="flex flex-col items-center gap-2">
        <div className="w-16 h-10 bg-[#1019EC] rounded-lg flex items-center justify-center -rotate-12 translate-x-2">
          <span className="text-white text-[8px]">transform</span>
        </div>
      </div>
    </SectionShell>
  );
}
