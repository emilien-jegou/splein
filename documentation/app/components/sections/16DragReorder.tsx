import SectionShell from "../layout/SectionShell";

export default function DragReorder() {
  return (
    <SectionShell
      number="16"
      title="Drag to Reorder"
      description="FLIP-animated drag-to-reorder with use_flip, use_follow, on:drag-start/step/end, and keyed reconciliation."
      code={`let flip_sig = use_flip(\n    row_ref.clone(),\n    drag.revision.map(Trigger::from),\n    SETTLE,\n);\n\nview! {\n    <Group\n        transform={displayed}\n        z_index={elevation}\n        on:pointer-down={move |event| {\n            drag.held.set(Some(label.clone()));\n        }}\n        on:drag-step={move |info| {\n            drag.want_centre.set(Some(centre));\n            if let Some(to) = drag.target_slot(centre) {\n                drag.move_row(from, to);\n            }\n        }}\n        on:drag-end={move |info| {\n            follow_handle.resolve_now();\n            settle.set(displayed.get());\n        }}\n    >\n        <Text size={14.0} color={Color::WHITE}>{label}</Text>\n    </Group>\n}`}
    >
      <div className="flex flex-col items-center gap-1">
        <div className="w-20 h-6 bg-[#1019EC] rounded flex items-center justify-center cursor-grab">
          <span className="text-white text-[7px]">1 Baseline</span>
        </div>
        <div className="w-20 h-6 bg-[#1019EC]/80 rounded flex items-center justify-center cursor-grab">
          <span className="text-white text-[7px]">2 Underline</span>
        </div>
        <div className="w-20 h-6 bg-[#1019EC]/60 rounded flex items-center justify-center cursor-grab">
          <span className="text-white text-[7px]">3 Strike</span>
        </div>
      </div>
    </SectionShell>
  );
}
