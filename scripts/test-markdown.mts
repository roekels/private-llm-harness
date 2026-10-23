// Markdown parser checks: npm test (Node 22 runs TypeScript directly).
import { parse, inline } from "../src/lib/markdown.ts";
const j = (x: unknown) => JSON.stringify(x);
const cases: [string, (b: any) => boolean][] = [
  ["# Title\n\nHello **bold** and *it* and `code`.", (b) => b[0].t === "h" && b[1].c.some((n: any) => n.t === "strong") && b[1].c.some((n: any) => n.t === "em") && b[1].c.some((n: any) => n.t === "code")],
  ["- a\n- b\n  - nested\n- c", (b) => b[0].t === "list" && b[0].items.length === 3 && b[0].items[1].c[1]?.t === "list"],
  ["1. one\n2. two\n\n3. three", (b) => b[0].t === "list" && b[0].ordered && b[0].items.length === 3],
  ["| a | b |\n|:--|--:|\n| 1 | 2 |\n| 3 | 4 |", (b) => b[0].t === "table" && b[0].rows.length === 2 && b[0].align[1] === "right"],
  ["```py\nprint(1)\n```\nafter", (b) => b[0].t === "code" && b[0].lang === "py" && b[1].t === "p"],
  ["```js\nstreaming", (b) => b[0].t === "code" && b[0].v === "streaming"],
  ["> quote\n> more\n\npara", (b) => b[0].t === "quote" && b[1].t === "p"],
  ["- [x] done\n- [ ] todo", (b) => b[0].items[0].task === true && b[0].items[1].task === false],
  ["see [site](https://example.com) and https://a.b/c.", (b) => b[0].c.filter((n: any) => n.t === "link").length === 2 && b[0].c.at(-1).v === "."],
  ["[x](javascript:alert(1))", (b) => !b[0].c.some((n: any) => n.t === "link")],
  ["<script>alert(1)</script>", (b) => b[0].c[0].t === "text" && b[0].c[0].v.includes("<script>")],
  ["snake_case_name here", (b) => b[0].c.length === 1 && b[0].c[0].t === "text"],
  ["line one\nline two", (b) => b[0].c.some((n: any) => n.t === "br")],
  ["---", (b) => b[0].t === "hr"],
  ["Text\n- item after text", (b) => b[0].t === "p" && b[1].t === "list"],
];
let fail = 0;
for (const [src, ok] of cases) {
  const b = parse(src);
  if (!ok(b)) { fail++; console.log("FAIL", j(src), "\n  ", j(b)); }
}
console.log(`markdown: ${cases.length - fail}/${cases.length} passed`);
if (fail) process.exit(1);
