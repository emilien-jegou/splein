export default function TextLayoutDemo() {
  return (
    <div className="flex w-full max-w-96 flex-col items-start gap-1 text-left">
      <span className="text-2xl font-bold text-black">Typography</span>
      <span className="text-sm font-light text-black">Light weight 300</span>
      <span className="text-sm font-bold text-black">Bold weight 700</span>
      <span className="text-sm text-black italic">Italic face</span>
      <span className="text-sm text-black underline">Underlined</span>
      <span className="text-sm text-black line-through">Struck</span>
      <span className="text-sm text-black overline">Overlined</span>
      <span className="text-sm tracking-widest text-black">Wide tracking</span>
    </div>
  );
}
