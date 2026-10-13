import { Highlight, themes } from "prism-react-renderer";

interface CodeBlockProps {
  code: string;
}

export default function CodeBlock({ code }: CodeBlockProps) {
  return (
    <Highlight theme={themes.vsLight} code={code} language="rust">
      {({ tokens, getLineProps, getTokenProps }) => (
        <pre className="text-[14px] leading-[110%] whitespace-pre-wrap font-['CommitMonoV143-200Italic','CommitMonoV143',system-ui,sans-serif] [font-variation-settings:'wght'_400,'ital'_0]">
          {tokens.map((line, i) => (
            <div key={i} {...getLineProps({ line })}>
              {line.map((token, key) => (
                <span key={key} {...getTokenProps({ token })} />
              ))}
            </div>
          ))}
        </pre>
      )}
    </Highlight>
  );
}
