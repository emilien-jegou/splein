import SectionShell from "../layout/SectionShell";

export default function HotReload() {
  return (
    <SectionShell
      number="30"
      title="Hot Reload"
      description="RemoteReloadExtension and define_hot_ui! for live editing. Rebuild the UI cdylib without restarting the app."
      code={`// In your UI crate (cdylib)\nextern crate aurora_shared as aurora;\n\n#[path = "../view.rs"]\nmod view;\n\naurora::define_hot_ui!(view::Page);\n\n// In your main app\nlet config = DevLoopConfig {\n    // watch paths, reload strategy\n};\nlet reloader = RemoteReloadExtension::new(config);\n\ncli.extension(reloader).run(move || build_ui())`}
    >
      <div className="flex flex-col items-center gap-1">
        <div className="flex gap-1">
          <span className="px-1 py-0.5 bg-orange-100 rounded text-[6px] text-orange-700">edit view.rs</span>
          <span className="text-[7px]">→</span>
          <span className="px-1 py-0.5 bg-green-100 rounded text-[6px] text-green-700">auto-reload</span>
        </div>
      </div>
    </SectionShell>
  );
}
