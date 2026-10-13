export default function ShadowsDemo() {
  const boxes = [
    { label: "web_like", shadow: "3px 6px 6px rgba(0,0,0,0.35)" },
    { label: "layered", shadow: "0 8px 16px rgba(122,61,143,0.15)" },
    { label: "elevation", shadow: "0 12px 24px rgba(0,0,0,0.2)" },
  ];
  return (
    <div className="flex items-center gap-8">
      {boxes.map((b) => (
        <div key={b.label} className="flex flex-col items-center gap-2">
          <div
            className="flex h-16 w-16 items-center justify-center rounded-xl bg-white"
            style={{ boxShadow: b.shadow }}
          >
            <div className="h-6 w-6 rounded bg-[#2B3A4D]" />
          </div>
          <span className="text-[9px] text-[#616161]">{b.label}</span>
        </div>
      ))}
    </div>
  );
}
