const LOREM =
  "Select any part of this paragraph. The engine computes glyph boxes and hit-tests the selection against them, so highlighting tracks the text exactly — try dragging across a line.";

export default function SelectableTextDemo() {
  return (
    <p className="max-w-112 select-text text-left text-sm leading-6 text-black">
      {LOREM}
    </p>
  );
}
