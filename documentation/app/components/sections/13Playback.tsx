import SectionShell from "../layout/SectionShell";

export default function Playback() {
  return (
    <SectionShell
      number="13"
      title="Playback"
      description="Self-driving animation with use_playback. Supports pause, resume, restart via ControllerHandle."
      code={`let (slide, control) = use_playback(\n    Transition::ease(SLIDE_SECS, Ease::InOutCubic)\n        .repeat(Repeat::PingPong),\n    0.0,\n    TRAVEL_PX,\n);\n\nview! {\n    <Group\n        x={slide}\n        on:pointer-enter={move |_| control.pause()}\n        on:pointer-leave={move |_| control.resume()}\n    >\n        <Text size={15.0} color={Color::WHITE}>"pause on hover"</Text>\n    </Group>\n}`}
    >
      <div className="flex flex-col items-center gap-2">
        <div className="w-10 h-10 bg-[#1019EC] rounded-lg flex items-center justify-center animate-bounce">
          <span className="text-white text-[7px]">play</span>
        </div>
        <span className="text-[8px] text-gray-400">hover to pause</span>
      </div>
    </SectionShell>
  );
}
