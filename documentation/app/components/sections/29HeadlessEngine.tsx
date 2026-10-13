import SectionShell from "../layout/SectionShell";

export default function HeadlessEngine() {
  return (
    <SectionShell
      number="29"
      title="Headless Engine"
      description="Engine::headless and HeadlessRaster for server-side rendering. Draw to pixel buffers without a window."
      code={`let engine = Engine::headless();\nlet raster = HeadlessRaster::new(&engine);\n\n// Benchmark: time draw_engine vs engine.frame()\nfor _ in 0..300 {\n    raster.draw_engine(&engine);\n    // or\n    engine.frame();\n}`}
    >
      <div className="flex flex-col items-center gap-1">
        <div className="px-2 py-1 bg-gray-800 rounded">
          <span className="text-[7px] text-green-400 font-mono">$ cargo run --release --example render_bench</span>
        </div>
      </div>
    </SectionShell>
  );
}
