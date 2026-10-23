// A small Markdown parser for model replies. It returns a tree that
// Markdown.svelte renders as elements and text nodes. Never HTML: model and web
// output is untrusted, so raw HTML in a reply shows as plain text.
//
// Supports: headings, paragraphs (single newlines kept as line breaks), fenced
// code, block quotes, bullet, numbered and task lists (nested), tables, rules,
// **bold**, *italic*, ~~strike~~, `code`, [links](https://...) and bare URLs.

export type Inline =
  | { t: "text"; v: string }
  | { t: "code"; v: string }
  | { t: "strong" | "em" | "del"; c: Inline[] }
  | { t: "link"; href: string; c: Inline[] }
  | { t: "br" };

export type Align = "left" | "center" | "right" | null;

export type Block =
  | { t: "p"; c: Inline[] }
  | { t: "h"; level: number; c: Inline[] }
  | { t: "code"; lang: string; v: string }
  | { t: "quote"; c: Block[] }
  | { t: "list"; ordered: boolean; start: number; items: { task: boolean | null; c: Block[] }[] }
  | { t: "table"; align: Align[]; head: Inline[][]; rows: Inline[][][] }
  | { t: "hr" };

const FENCE = /^ {0,3}(`{3,}|~{3,})\s*([\w#+.-]*)/;
const HEADING = /^ {0,3}(#{1,6})\s+(.*?)\s*#*\s*$/;
const HR = /^ {0,3}([-*_])(\s*\1){2,}\s*$/;
const ITEM = /^(\s*)([-*+]|\d{1,9}[.)])\s+(.*)$/;
const QUOTE = /^ {0,3}>\s?(.*)$/;
const TABLE_SEP = /^\s*\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)*\|?\s*$/;

const indentOf = (s: string) => s.length - s.trimStart().length;
const blank = (s: string) => s.trim() === "";

/** Only these open on click (through Rust, in the default browser). */
export function safeHref(href: string): string | null {
  try {
    const u = new URL(href);
    return (u.protocol === "https:" || u.protocol === "http:") && !u.username && !u.password ? u.href : null;
  } catch {
    return null;
  }
}

export function parse(src: string): Block[] {
  return blocks(src.replace(/\r\n?/g, "\n").replace(/\t/g, "    ").split("\n"));
}

function blocks(lines: string[]): Block[] {
  const out: Block[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i] ?? "";
    if (blank(line)) {
      i++;
      continue;
    }

    const fence = FENCE.exec(line);
    if (fence) {
      const mark = fence[1] ?? "```";
      const body: string[] = [];
      i++;
      while (i < lines.length && !(lines[i] ?? "").trimStart().startsWith(mark)) body.push(lines[i++] ?? "");
      i++; // closing fence (or past the end while streaming)
      out.push({ t: "code", lang: fence[2] ?? "", v: body.join("\n") });
      continue;
    }

    const h = HEADING.exec(line);
    if (h) {
      out.push({ t: "h", level: (h[1] ?? "#").length, c: inline(h[2] ?? "") });
      i++;
      continue;
    }

    if (HR.test(line)) {
      out.push({ t: "hr" });
      i++;
      continue;
    }

    if (QUOTE.test(line)) {
      const inner: string[] = [];
      while (i < lines.length && !blank(lines[i] ?? "") && (QUOTE.test(lines[i] ?? "") || inner.length)) {
        const m = QUOTE.exec(lines[i] ?? "");
        if (!m && (ITEM.test(lines[i] ?? "") || FENCE.test(lines[i] ?? ""))) break;
        inner.push(m ? (m[1] ?? "") : (lines[i] ?? ""));
        i++;
      }
      out.push({ t: "quote", c: blocks(inner) });
      continue;
    }

    if (ITEM.test(line)) {
      i = list(lines, i, out);
      continue;
    }

    if (line.includes("|") && TABLE_SEP.test(lines[i + 1] ?? "") && (lines[i + 1] ?? "").includes("-")) {
      const head = cells(line);
      const align = cells(lines[i + 1] ?? "").map((c): Align => {
        const l = c.startsWith(":");
        const r = c.endsWith(":");
        return l && r ? "center" : r ? "right" : l ? "left" : null;
      });
      i += 2;
      const rows: Inline[][][] = [];
      while (i < lines.length && !blank(lines[i] ?? "") && (lines[i] ?? "").includes("|")) {
        rows.push(cells(lines[i] ?? "").map(inline));
        i++;
      }
      out.push({ t: "table", align, head: head.map(inline), rows });
      continue;
    }

    // Paragraph: until a blank line or the start of another block.
    const para: string[] = [];
    while (i < lines.length) {
      const l = lines[i] ?? "";
      if (blank(l) || (para.length && (FENCE.test(l) || HEADING.test(l) || QUOTE.test(l) || HR.test(l) || ITEM.test(l)))) break;
      para.push(l.trim());
      i++;
    }
    out.push({ t: "p", c: inline(para.join("\n")) });
  }
  return out;
}

/** Parses one list starting at `start`. Returns the next line index. */
function list(lines: string[], start: number, out: Block[]): number {
  const first = ITEM.exec(lines[start] ?? "");
  const base = indentOf(lines[start] ?? "");
  const ordered = /\d/.test(first?.[2] ?? "");
  const items: { task: boolean | null; c: Block[] }[] = [];
  let i = start;
  const sameList = (l: string | undefined) => {
    const m = l === undefined ? null : ITEM.exec(l);
    return !!m && indentOf(l ?? "") === base && /\d/.test(m[2] ?? "") === ordered;
  };
  while (i < lines.length) {
    // A blank line between items keeps one list ("1." blank "2.").
    if (blank(lines[i] ?? "") && items.length) {
      let k = i;
      while (k < lines.length && blank(lines[k] ?? "")) k++;
      if (sameList(lines[k])) i = k;
      else break;
    }
    const m = ITEM.exec(lines[i] ?? "");
    if (!m || !sameList(lines[i])) break;
    const contentAt = (m[1] ?? "").length + (m[2] ?? "").length + 1;
    let text = m[3] ?? "";
    let task: boolean | null = null;
    const tm = /^\[([ xX])\]\s+(.*)$/.exec(text);
    if (tm) {
      task = tm[1] !== " ";
      text = tm[2] ?? "";
    }
    const body = [text];
    i++;
    // Continuation: indented past the marker, or lazy text right after it.
    while (i < lines.length) {
      const l = lines[i] ?? "";
      if (blank(l)) {
        const next = lines.slice(i + 1).find((x) => !blank(x));
        if (next !== undefined && indentOf(next) > base && !(ITEM.test(next) && indentOf(next) <= base)) {
          body.push("");
          i++;
          continue;
        }
        break;
      }
      const ind = indentOf(l);
      if (ind > base) {
        body.push(l.slice(Math.min(ind, contentAt)));
        i++;
      } else if (!ITEM.test(l) && !FENCE.test(l) && !HEADING.test(l) && !QUOTE.test(l) && body.at(-1) !== "") {
        body.push(l.trim());
        i++;
      } else break;
    }
    items.push({ task, c: blocks(body) });
  }
  const startNum = ordered ? Number.parseInt(first?.[2] ?? "1", 10) || 1 : 1;
  out.push({ t: "list", ordered, start: startNum, items });
  return i;
}

function cells(row: string): string[] {
  let s = row.trim();
  if (s.startsWith("|")) s = s.slice(1);
  if (s.endsWith("|") && !s.endsWith("\\|")) s = s.slice(0, -1);
  const out: string[] = [];
  let cur = "";
  for (let i = 0; i < s.length; i++) {
    if (s[i] === "\\" && s[i + 1] === "|") {
      cur += "|";
      i++;
    } else if (s[i] === "|") {
      out.push(cur.trim());
      cur = "";
    } else cur += s[i];
  }
  out.push(cur.trim());
  return out;
}

const ESCAPABLE = /[\\`*_{}[\]()#+\-.!~|>]/;
const URL_AT = /^https?:\/\/[^\s<>]+/;

export function inline(s: string): Inline[] {
  const out: Inline[] = [];
  let buf = "";
  const flush = () => {
    if (buf) out.push({ t: "text", v: buf });
    buf = "";
  };
  let i = 0;
  while (i < s.length) {
    const ch = s[i] ?? "";
    const prev = s[i - 1] ?? " ";

    if (ch === "\\" && ESCAPABLE.test(s[i + 1] ?? "")) {
      buf += s[i + 1];
      i += 2;
      continue;
    }
    if (ch === "\n") {
      flush();
      out.push({ t: "br" });
      i++;
      continue;
    }
    if (ch === "`") {
      let n = 1;
      while (s[i + n] === "`") n++;
      const tick = "`".repeat(n);
      const end = s.indexOf(tick, i + n);
      if (end > i + n - 1) {
        flush();
        const v = s.slice(i + n, end);
        out.push({ t: "code", v: v.length > 2 && v.startsWith(" ") && v.endsWith(" ") ? v.slice(1, -1) : v });
        i = end + n;
        continue;
      }
    }
    if ((ch === "*" || ch === "_") && s[i + 1] === ch) {
      const end = closing(s, ch + ch, i + 2);
      if (end > 0 && (ch === "*" || !/\w/.test(prev))) {
        flush();
        out.push({ t: "strong", c: inline(s.slice(i + 2, end)) });
        i = end + 2;
        continue;
      }
    }
    if ((ch === "*" || ch === "_") && s[i + 1] !== ch && !/\s/.test(s[i + 1] ?? " ")) {
      const end = closing(s, ch, i + 1);
      if (end > 0 && (ch === "*" || !/\w/.test(prev))) {
        flush();
        out.push({ t: "em", c: inline(s.slice(i + 1, end)) });
        i = end + 1;
        continue;
      }
    }
    if (ch === "~" && s[i + 1] === "~") {
      const end = closing(s, "~~", i + 2);
      if (end > 0) {
        flush();
        out.push({ t: "del", c: inline(s.slice(i + 2, end)) });
        i = end + 2;
        continue;
      }
    }
    if (ch === "[") {
      const close = matching(s, i);
      if (close > 0 && s[close + 1] === "(") {
        const paren = s.indexOf(")", close + 2);
        const raw = paren > 0 ? s.slice(close + 2, paren).trim().split(/\s+/)[0] ?? "" : "";
        const href = raw.replace(/^<|>$/g, "");
        if (paren > 0 && href) {
          flush();
          const label = inline(s.slice(i + 1, close));
          const safe = safeHref(href);
          if (safe) out.push({ t: "link", href: safe, c: label });
          else out.push(...label, { t: "text", v: ` (${href})` });
          i = paren + 1;
          continue;
        }
      }
    }
    if ((ch === "h" || ch === "<") && !/\w/.test(prev)) {
      const rest = s.slice(ch === "<" ? i + 1 : i);
      const m = URL_AT.exec(rest);
      if (m) {
        let url = m[0];
        if (ch === "<") url = url.replace(/>.*$/, "");
        // Trailing punctuation belongs to the sentence, not the URL.
        while (/[.,;:!?'")\]]$/.test(url) && !(url.endsWith(")") && url.includes("("))) url = url.slice(0, -1);
        const safe = safeHref(url);
        if (safe) {
          flush();
          out.push({ t: "link", href: safe, c: [{ t: "text", v: url }] });
          i += url.length + (ch === "<" && s[i + 1 + url.length] === ">" ? 2 : 0);
          continue;
        }
      }
    }
    buf += ch;
    i++;
  }
  flush();
  return out;
}

/** Index of the closing `mark` with non-empty, non-space-padded content, or -1. */
function closing(s: string, mark: string, from: number): number {
  let j = s.indexOf(mark, from);
  while (j > 0) {
    const inner = s.slice(from, j);
    const after = s[j + mark.length] ?? " ";
    const okEnd = mark[0] !== "_" || !/\w/.test(after);
    if (inner.trim() && !/\s$/.test(inner) && !inner.includes("\n\n") && okEnd) return j;
    j = s.indexOf(mark, j + 1);
  }
  return -1;
}

function matching(s: string, open: number): number {
  let depth = 0;
  for (let j = open; j < s.length; j++) {
    if (s[j] === "\\") j++;
    else if (s[j] === "[") depth++;
    else if (s[j] === "]" && --depth === 0) return j;
    else if (s[j] === "\n" && s[j + 1] === "\n") return -1;
  }
  return -1;
}
