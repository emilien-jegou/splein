import SectionShell from "../layout/SectionShell";

export default function PressFeedback() {
  return (
    <SectionShell
      number="15"
      title="Press Feedback"
      description="Press-and-release with asymmetric transitions. on:pointer-down, on:pointer-up, on:pointer-leave."
      code={`view! {\n    <Group\n        height={52.0}\n        radius="max"\n        fill={ACCENT}\n        scale={active}\n        on:pointer-down={move |_| down.set(true)}\n        on:pointer-up={move |_| up.set(false)}\n        on:pointer-leave={move |_| drop.set(false)}\n        on:click={move |info| {\n            counted.update(|n| *n += info.click_count as u32)\n        }}\n    >\n        <Text size={15.0} weight={700u16} color={Color::WHITE}>"Continue"</Text>\n    </Group>\n}`}
    >
      <div className="flex flex-col items-center gap-2">
        <div className="px-4 py-2 bg-[#1019EC] rounded-full scale-90">
          <span className="text-white text-[10px]">pressed</span>
        </div>
        <div className="px-4 py-2 bg-[#1019EC] rounded-full scale-100">
          <span className="text-white text-[10px]">normal</span>
        </div>
      </div>
    </SectionShell>
  );
}
