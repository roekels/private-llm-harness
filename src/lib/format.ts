// Splits model output into text and fenced code blocks. Output is rendered as
// text nodes only, never as HTML, because model output is untrusted.

export type Segment = { kind: "text"; text: string } | { kind: "code"; lang: string; text: string };

const FENCE = /```([\w#+.-]*)[^\n]*\n([\s\S]*?)(?:```|$)/g;

export function segments(src: string): Segment[] {
  const out: Segment[] = [];
  let last = 0;
  const text = (t: string) => {
    // Blank lines next to a code block are already covered by its margin.
    const trimmed = (out.at(-1)?.kind === "code" ? t.replace(/^\n+/, "") : t).replace(/\n+$/, "");
    if (trimmed) out.push({ kind: "text", text: trimmed });
  };
  for (const m of src.matchAll(FENCE)) {
    const start = m.index ?? 0;
    if (start > last) text(src.slice(last, start));
    out.push({ kind: "code", lang: m[1] ?? "", text: (m[2] ?? "").replace(/\n$/, "") });
    last = start + m[0].length;
  }
  if (last < src.length) {
    const rest = src.slice(last);
    // Keep trailing newlines at the very end while streaming, so text doesn't jump.
    const t = out.at(-1)?.kind === "code" ? rest.replace(/^\n+/, "") : rest;
    if (t) out.push({ kind: "text", text: t });
  }
  return out;
}

/** Splits **bold** runs out of plain text. Rendered as text nodes, never HTML. */
export function boldRuns(text: string): { bold: boolean; text: string }[] {
  const out: { bold: boolean; text: string }[] = [];
  let last = 0;
  for (const m of text.matchAll(/\*\*([^*\n]+?)\*\*/g)) {
    const i = m.index ?? 0;
    if (i > last) out.push({ bold: false, text: text.slice(last, i) });
    out.push({ bold: true, text: m[1] ?? "" });
    last = i + m[0].length;
  }
  if (last < text.length) out.push({ bold: false, text: text.slice(last) });
  return out;
}

/** The VM firewall status ("on", "on limit=20", "off", ...) in words. */
export function firewallText(network?: string | null): string {
  if (!network) return "unknown";
  const [mode, ...rest] = network.split(" ");
  const limit = rest.find((r) => r.startsWith("limit="))?.slice(6);
  if (mode === "on") return limit ? `internet only, max ${limit} new connections/s per site` : "internet only";
  if (mode === "off") return "no network";
  return network;
}

const num = new Intl.NumberFormat("en-US");
export const fmtInt = (n: number) => num.format(Math.round(n));
