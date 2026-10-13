import { ReactNode } from "react";

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
    <div className="[font-synthesis:none] wrap-anywhere flex items-start gap-16 justify-between p-16 self-stretch border-b border-b-solid border-b-[#00000014] antialiased">
      <div className="flex flex-col items-start gap-3">
        <div className="text-[28px] leading-[110%] shrink-0 w-fit font-['Inter-Medium','Inter',system-ui,sans-serif] font-medium text-black">
          {number} — {title}
        </div>
        <div className="text-[17px] leading-[150%] w-132 shrink-0 font-['Inter-Regular','Inter',system-ui,sans-serif] text-[#616161]">
          {description}
        </div>
      </div>
      <div className="flex flex-col items-start gap-3">
        <div className="text-[14px] leading-[110%] whitespace-pre-wrap font-['CommitMonoV143-200Italic','CommitMonoV143',system-ui,sans-serif] [font-variation-settings:'wght'_400,'ital'_0] text-black">
          {code}
        </div>
      </div>
      <div className="flex flex-col items-center gap-3 h-71.5 w-126.75 justify-center rounded-[15px] shrink-0 bg-[#F5F5F5]">
        {children}
      </div>
    </div>
  );
}
