import SectionShell from "../layout/SectionShell";

export default function Springs() {
  return (
    <SectionShell
      number="14"
      title="Springs"
      description="Physics-based springs with Spring::new(stiffness, damping, mass). Retargets mid-flight without popping."
      code={`// Custom spring\nconst SETTLE: Spring = Spring::new(300.0, 38.0, 1.0);\n\n// Named presets\nTransition::from_str("spring")\nTransition::from_str("spring:bouncy")\nTransition::from_str("spring:smooth")\n\n// Retargeting: adopts live position + velocity\nlet step = use_signal(0u32);\nlet target = step.map(|n| if n % 2 == 0 { RIGHT } else { LEFT });\nlet x = Transition::from_str("spring").on(target);`}
    >
      <div className="flex flex-col items-center gap-2">
        <div className="w-8 h-8 bg-[#10B981] rounded-full flex items-center justify-center">
          <span className="text-white text-[6px]">spring</span>
        </div>
        <span className="text-[8px] text-gray-400">stiffness: 300</span>
      </div>
    </SectionShell>
  );
}
