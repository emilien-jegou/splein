import { ReactNode } from "react";
import CodeBlock from "../CodeBlock";

interface SectionShellProps {
  number: string;
  title: string;
  description: string;
  code: string;
  children: ReactNode;
}

export default function SectionShell({
  number,
  title,
  description,
  code,
  children,
}: SectionShellProps) {
  return (
    <div className="[font-synthesis:none] wrap-anywhere flex flex-col items-start gap-10 p-16 self-stretch border-b border-b-solid border-b-[#00000014] antialiased">
      <div className="flex flex-wrap items-start justify-between gap-8 self-stretch">
        <div className="text-[28px] leading-[110%] w-fit shrink-0 font-['Inter-Medium','Inter',system-ui,sans-serif] font-medium text-black">
          {number} — {title}
        </div>
        <div className="text-[17px] leading-[150%] w-132 shrink-0 font-['Inter-Regular','Inter',system-ui,sans-serif] text-[#616161]">
          {description}
        </div>
      </div>
      <div className="flex flex-wrap items-start gap-12 self-stretch">
        <div className="w-120 shrink-0">
          <CodeBlock code={code} />
        </div>
        <div className="flex-1 min-w-0 h-80 rounded-[15px] bg-[#F5F5F5] overflow-hidden flex items-center justify-center">
          {children}
        </div>
      </div>
    </div>
  );
}
