export default function VectorGraphicDemo() {
  return (
    <div className="flex flex-col items-center gap-3">
      <svg
        width="120"
        height="96"
        viewBox="0 0 120 96"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
      >
        <rect width="120" height="96" rx="12" fill="#24405E" />
        <circle cx="60" cy="40" r="18" fill="#FF00FF" />
        <path
          d="M30 78 Q60 50 90 78"
          stroke="#FFFFFF"
          strokeWidth="3"
          strokeLinecap="round"
        />
      </svg>
      <span className="text-[10px] text-[#616161]">
        VectorGraphic::from_str(SVG)
      </span>
    </div>
  );
}
