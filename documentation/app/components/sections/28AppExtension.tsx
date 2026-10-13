import SectionShell from "../layout/SectionShell";

export default function AppExtension() {
  return (
    <SectionShell
      number="28"
      title="App Extension"
      description="Engine lifecycle hooks with AppExtension trait. on_frame for per-frame updates, on_event for window events, on_update for polling."
      code={`impl AppExtension for Ticker {\n    fn on_frame(&mut self, _diagnostics: &FrameDiagnostics) {\n        let elapsed = self.start.elapsed().as_secs_f32();\n        self.time_sig.set(elapsed);\n    }\n\n    fn on_event(&mut self, event: &WindowEvent) -> bool {\n        if let WindowEvent::Resized(size) = event {\n            self.win_w = size.width as f32;\n            return true;\n        }\n        false\n    }\n\n    fn on_update(&mut self, engine: &mut Engine) -> bool {\n        let text = engine.combined_selected_text();\n        // ...\n    }\n}`}
    >
      <div className="flex flex-col items-center gap-1">
        <div className="flex gap-1">
          <span className="px-1 py-0.5 bg-blue-100 rounded text-[6px] text-blue-700">on_frame</span>
          <span className="px-1 py-0.5 bg-green-100 rounded text-[6px] text-green-700">on_event</span>
          <span className="px-1 py-0.5 bg-purple-100 rounded text-[6px] text-purple-700">on_update</span>
        </div>
      </div>
    </SectionShell>
  );
}
