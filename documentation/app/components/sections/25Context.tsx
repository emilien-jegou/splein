import SectionShell from "../layout/SectionShell";

export default function Context() {
  return (
    <SectionShell
      number="25"
      title="Context"
      description="Dependency injection with Context<T>, initContext!, and Provider. Share state without prop drilling."
      code={`static STATUS: Context<String> = initContext!();\nstatic CLIPBOARD: Context<String> = initContext!();\n\nview! {\n    <Provider grant={STATUS.provide(status)}>\n        <Provider grant={CLIPBOARD.provide(clipboard)}>\n            <Page />\n        </Provider>\n    </Provider>\n}\n\n// Consume anywhere in the tree\n<Text size={12.0} color={INK}>{STATUS.get()}</Text>`}
    >
      <div className="flex flex-col items-center gap-1">
        <div className="px-2 py-1 bg-purple-100 rounded">
          <span className="text-[7px] text-purple-700">Provider</span>
        </div>
        <span className="text-[6px] text-gray-400">↓</span>
        <div className="px-2 py-1 bg-green-100 rounded">
          <span className="text-[7px] text-green-700">Consumer</span>
        </div>
      </div>
    </SectionShell>
  );
}
