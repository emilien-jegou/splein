const COPY =
  "The quick brown fox jumps over the lazy dog while the engine lays out every glyph with subpixel precision.";

export default function TextWrapDemo() {
  return (
    <div className="flex w-full max-w-112 flex-col items-start gap-4 text-left">
      <div>
        <div className="mb-1 text-[9px] font-semibold tracking-wide text-[#2563EB] uppercase">
          Wrap (default)
        </div>
        <p className="max-w-96 text-xs leading-5 text-black">{COPY}</p>
      </div>
      <div>
        <div className="mb-1 text-[9px] font-semibold tracking-wide text-[#2563EB] uppercase">
          NoWrap · clipped
        </div>
        <p className="max-w-96 text-xs leading-5 text-black whitespace-nowrap overflow-hidden">
          {COPY}
        </p>
      </div>
      <div>
        <div className="mb-1 text-[9px] font-semibold tracking-wide text-[#2563EB] uppercase">
          FlowWrap · flex-like
        </div>
        <div className="flex max-w-96 flex-wrap gap-1.5">
          {["Submit", "Cancel the whole thing", "OK", "Dismiss"].map((c) => (
            <span
              key={c}
              className="rounded bg-[#2B3A4D] px-2 py-1 text-[10px] text-white"
            >
              {c}
            </span>
          ))}
        </div>
      </div>
    </div>
  );
}
