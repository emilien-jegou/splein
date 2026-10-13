export default function GroupDemo() {
  return (
    <div className="flex flex-col items-center gap-3">
      <div className="flex h-28 w-full max-w-112 flex-col items-center justify-center gap-2 rounded-lg bg-white p-3">
        <div className="flex h-16 w-40 items-center justify-center rounded bg-[#336133]">
          <span className="text-xs text-white">500 × 204</span>
        </div>
        <div className="h-6 w-40 rounded bg-[#CA6D60] flex items-center justify-center">
          <span className="text-[9px] text-white">fill</span>
        </div>
      </div>
      <span className="text-[10px] text-[#616161]">
        direction · width · height · fill · gap
      </span>
    </div>
  );
}
