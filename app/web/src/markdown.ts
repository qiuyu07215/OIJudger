// Markdown statements with $inline$ / $$display$$ math (KaTeX), sanitized.
import MarkdownIt from "markdown-it";
import katex from "katex";
import DOMPurify from "dompurify";
import "katex/dist/katex.min.css";

function tex(src: string, display: boolean) {
  try {
    return katex.renderToString(src, { displayMode: display, throwOnError: false, output: "html" });
  } catch {
    return `<code>${src.replace(/</g, "&lt;")}</code>`;
  }
}

// markdown-it's rule state types are not exported in a stable place.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
type Any = any;

function mathPlugin(md: InstanceType<typeof MarkdownIt>) {
  // $$ ... $$ blocks (may span lines).
  md.block.ruler.before("fence", "math_block", (state: Any, start: number, end: number, silent: boolean) => {
    const pos = state.bMarks[start] + state.tShift[start];
    const max = state.eMarks[start];
    if (state.src.slice(pos, pos + 2) !== "$$") return false;
    if (silent) return true;
    let line = start;
    let content = state.src.slice(pos + 2, max);
    let closed = content.trimEnd().endsWith("$$");
    if (closed) content = content.trimEnd().slice(0, -2);
    while (!closed && ++line < end) {
      const s = state.src.slice(state.bMarks[line] + state.tShift[line], state.eMarks[line]);
      if (s.trimEnd().endsWith("$$")) {
        content += "\n" + s.trimEnd().slice(0, -2);
        closed = true;
      } else {
        content += "\n" + s;
      }
    }
    const token = state.push("math_block", "math", 0);
    token.content = content;
    token.map = [start, line + 1];
    state.line = line + 1;
    return true;
  });
  // $ ... $ inline (not "$ " and not "$$").
  md.inline.ruler.after("escape", "math_inline", (state: Any, silent: boolean) => {
    const src = state.src;
    const start = state.pos;
    if (src[start] !== "$" || src[start + 1] === "$" || src[start + 1] === " ") return false;
    let end = start + 1;
    while (end < src.length) {
      if (src[end] === "\\") {
        end += 2;
        continue;
      }
      if (src[end] === "$") break;
      end++;
    }
    if (end >= src.length || src[end - 1] === " ") return false;
    if (!silent) {
      const token = state.push("math_inline", "math", 0);
      token.content = src.slice(start + 1, end);
    }
    state.pos = end + 1;
    return true;
  });
  md.renderer.rules.math_inline = (tokens: Any, i: number) => tex(tokens[i].content, false);
  md.renderer.rules.math_block = (tokens: Any, i: number) => `<div class="math-block">${tex(tokens[i].content, true)}</div>`;
}

const md = new MarkdownIt({ html: true, linkify: true, breaks: false });
md.use(mathPlugin);

/** Render a statement; relative image paths go through the problem's file endpoint. */
export function renderStatement(text: string, problem: string): string {
  const html = md.render(text);
  const clean = DOMPurify.sanitize(html, { ADD_ATTR: ["target"] });
  const div = document.createElement("div");
  div.innerHTML = clean;
  div.querySelectorAll("img").forEach((img) => {
    const src = img.getAttribute("src") || "";
    if (src && !/^(https?:|data:|\/)/i.test(src)) {
      const file = src.split(/[\\/]/).pop()!;
      img.setAttribute("src", `/api/problems/${encodeURIComponent(problem)}/files/${encodeURIComponent(file)}`);
    }
  });
  div.querySelectorAll("a[href^='http']").forEach((a) => a.setAttribute("target", "_blank"));
  return div.innerHTML;
}
