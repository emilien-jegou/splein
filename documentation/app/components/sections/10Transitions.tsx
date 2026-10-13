import SectionShell from "../layout/SectionShell";

export default function Transitions() {
  return (
    <SectionShell
      number="10"
      title="Transitions"
      description="Animate properties with Transition::from_str. Supports tween curves (out-cubic, out-quint) and springs."
      code={`// Tween: duration + curve\nlet tween = Transition::from_str("out-cubic 200ms");\nlet opacity = tween.on(enabled.map(|on| if on { 1.0 } else { 0.35 }));\nlet slide_x = tween.on(enabled.map(|on| if on { 0.0 } else { 48.0 }));\n\n// Spring: physics-based, no duration\nlet spring = Transition::from_str("spring");\nlet scale = spring.on(enabled.map(|on| if on { 1.18 } else { 1.0 }));\n\n// Named spring presets\nlet bouncy = Transition::from_str("spring:bouncy");\nlet smooth = Transition::from_str("spring:smooth");`}
    >
      <div className="flex flex-col items-center gap-2">
        <div className="w-16 h-8 bg-[#1019EC] rounded-lg flex items-center justify-center">
          <span className="text-white text-[8px]">tween</span>
        </div>
        <div className="w-12 h-12 bg-[#10B981] rounded-full flex items-center justify-center">
          <span className="text-white text-[7px]">spring</span>
        </div>
      </div>
    </SectionShell>
  );
}
