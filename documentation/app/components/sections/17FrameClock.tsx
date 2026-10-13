import SectionShell from "../layout/SectionShell";

export default function FrameClock() {
  return (
    <SectionShell
      number="17"
      title="Frame Clock"
      description="use_frame_clock for time-driven animation. use_interval and use_interval_toggle for periodic updates."
      code={`let time = use_frame_clock();\nlet travel = time.map(|t| {\n    ((t as f32) * SWEEP_HZ).sin() * TRAVEL\n});\n\n// Periodic toggle\nlet outward = use_interval_toggle(STEP_SECS);\n\n// Custom interval\nuse_interval(RETARGET_SECS, {\n    let step = step.clone();\n    move || step.update(|n| *n += 1)\n});`}
    >
      <div className="flex flex-col items-center gap-2">
        <div className="w-3 h-3 bg-[#EC4899] rounded-full animate-pulse" />
        <span className="text-[8px] text-gray-400">sin(t) × 120px</span>
      </div>
    </SectionShell>
  );
}
