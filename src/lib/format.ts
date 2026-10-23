// Small text helpers. Output is rendered as text nodes only, never as HTML.

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
