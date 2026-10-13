export default function FontFamilyDemo() {
  return (
    <div className="flex flex-col items-center gap-2">
      <span
        className="text-3xl text-black"
        style={{ fontFamily: "Inter, system-ui, sans-serif" }}
      >
        Custom font
      </span>
      <span
        className="text-lg text-[#616161]"
        style={{ fontFamily: "Inter, system-ui, sans-serif" }}
      >
        Inter variable
      </span>
      <span className="text-[10px] text-[#616161]">
        font_family="Inter" · registered at startup
      </span>
    </div>
  );
}
