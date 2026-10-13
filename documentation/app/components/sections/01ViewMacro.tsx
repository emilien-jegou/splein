import SectionShell from "../layout/SectionShell";

export default function ViewMacro() {
  return (
    <SectionShell
      number="01"
      title="view! Macro"
      description="The fundamental building block of every Aurora component. Declarative RSX syntax with typed props."
      code={`use aurora::prelude::*;\n\n#[component]\npub fn CounterButton() -> impl IntoElement {\n    let count = use_signal(0);\n    let label = count.map(|c| format!("Count: {c}"));\n\n    view! {\n        <Group\n            direction="horizontal"\n            radius={4.0}\n            padding={12.0}\n            fill="#2563eb"\n            on:click={|_| count.update(|c| *c += 1)}\n        >\n            <Text size={14.0} weight={600u16} color={Color::WHITE}>\n                {label}\n            </Text>\n        </Group>\n    }\n}`}
    >
      <div className="flex flex-col items-start rounded-sm p-3 bg-[#2563eb]">
        <div className="text-[14px] leading-[110%] text-pretty font-['CommitMonoV143-200Italic','CommitMonoV143',system-ui,sans-serif] [font-variation-settings:'wght'_400,'ital'_0] text-white">
          Count: 0
        </div>
      </div>
    </SectionShell>
  );
}
