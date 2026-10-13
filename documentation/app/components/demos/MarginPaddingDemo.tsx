export default function MarginPaddingDemo() {
  return (
    <div className="flex flex-col items-center gap-3">
      <div className="rounded border border-dashed border-[#2563eb]/50 p-6">
        <div className="rounded bg-[#2B3A4D] px-6 py-3">
          <span className="text-xs text-white">padding 8 14</span>
        </div>
      </div>
      <div className="flex items-center gap-4">
        <div className="h-8 w-8 rounded bg-[#CA6D60]" />
        <div className="h-8 w-8 rounded bg-[#CA6D60]" />
        <div className="h-8 w-8 rounded bg-[#CA6D60]" />
      </div>
      <span className="text-[10px] text-[#616161]">gap: 16</span>
    </div>
  );
}
