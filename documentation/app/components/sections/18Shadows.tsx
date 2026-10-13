import SectionShell from "../layout/SectionShell";

export default function Shadows() {
  return (
    <SectionShell
      number="18"
      title="Shadows"
      description="Shadow::web_like for CSS parity, Shadow::layered for stacked drops, Shadow::elevation for physical lighting."
      code={`// Web-like: offset_x, offset_y, blur, spread, color\nShadow::web_like(3.0, 6.0, 6.0, 0.0,\n    Color::rgba(0.0, 0.0, 0.0, 0.35))\n\n// Layered: stacked drop shadows\nShadow::layered(Color::rgba(122.0/255.0, 61.0/255.0, 143.0/255.0, 0.15))\n    .drop(8.0, 16.0, 16.0)\n    .drops(&[(0.5,1.0,1.0), (1.0,2.0,2.0), (2.0,4.0,4.0)])\n\n// Physical elevation\nShadow::elevation(12.0)\n    .spread(spread)\n    .color(tinted)\n    .falloff(FalloffCurve::EaseOut)`}
    >
      <div className="flex flex-col items-center gap-3">
        <div className="w-[75px] h-[75px] bg-white rounded-lg shadow-[3px_6px_6px_0px_rgba(0,0,0,0.35)]" />
        <div className="w-[75px] h-[75px] bg-white rounded-lg shadow-[0_4px_16px_0px_rgba(122,61,143,0.15)]" />
      </div>
    </SectionShell>
  );
}
