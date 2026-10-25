// Markdown de las respuestas de la IA (chat): lo que suelen escribir los
// modelos — títulos, negritas, cursivas, código, listas anidadas, citas,
// tablas y enlaces. Devuelve un árbol que el componente pinta con JSX (nunca
// HTML crudo: el texto de la IA no se inyecta como marcado)

export type Inline =
  | { t: "text"; v: string }
  | { t: "code"; v: string }
  | { t: "b"; c: Inline[] }
  | { t: "i"; c: Inline[] }
  | { t: "a"; href: string; c: Inline[] };

export type Align = "left" | "center" | "right" | undefined;

export type Block =
  | { t: "p"; c: Inline[] }
  | { t: "h"; level: number; c: Inline[] }
  | { t: "code"; lang: string; v: string }
  | { t: "list"; ordered: boolean; start: number; items: Block[][] }
  | { t: "quote"; c: Block[] }
  | { t: "hr" }
  | { t: "table"; align: Align[]; head: Inline[][]; rows: Inline[][][] };

const FENCE = /^\s{0,3}(`{3,}|~{3,})\s*([\w+-]*)/;
const HEADING = /^\s{0,3}(#{1,6})\s+(.*?)\s*#*\s*$/;
const HR = /^\s{0,3}([-*_])(\s*\1){2,}\s*$/;
const QUOTE = /^\s{0,3}>\s?/;
const ITEM = /^(\s*)([-*+]|(\d{1,9})[.)])\s+(.*)$/;
const TABLE_RULE = /^\s*\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)*\|?\s*$/;

const indentOf = (s: string) => s.length - s.trimStart().length;
const blank = (s: string) => s.trim() === "";

const cells = (row: string) => {
  let s = row.trim();
  if (s.startsWith("|")) s = s.slice(1);
  if (s.endsWith("|") && !s.endsWith("\\|")) s = s.slice(0, -1);
  return s.split(/(?<!\\)\|/).map((c) => c.trim().replace(/\\\|/g, "|"));
};

const isTable = (lines: string[], i: number) => lines[i].includes("|") && i + 1 < lines.length && lines[i + 1].includes("-") && TABLE_RULE.test(lines[i + 1]);

/** ¿Empieza aquí otro bloque? (corta un párrafo) */
const startsBlock = (lines: string[], i: number) => {
  const l = lines[i];
  return FENCE.test(l) || HEADING.test(l) || HR.test(l) || QUOTE.test(l) || ITEM.test(l) || isTable(lines, i);
};

export function parseMarkdown(text: string): Block[] {
  return blocks(text.replace(/\r\n?/g, "\n").split("\n"));
}

function blocks(lines: string[]): Block[] {
  const out: Block[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (blank(line)) {
      i++;
      continue;
    }
    const fence = FENCE.exec(line);
    if (fence) {
      const close = fence[1][0].repeat(fence[1].length);
      const body: string[] = [];
      i++;
      while (i < lines.length && !lines[i].trim().startsWith(close)) body.push(lines[i++]);
      i++;
      out.push({ t: "code", lang: fence[2], v: body.join("\n") });
      continue;
    }
    const heading = HEADING.exec(line);
    if (heading) {
      out.push({ t: "h", level: heading[1].length, c: parseInline(heading[2]) });
      i++;
      continue;
    }
    if (HR.test(line)) {
      out.push({ t: "hr" });
      i++;
      continue;
    }
    if (QUOTE.test(line)) {
      const body: string[] = [];
      while (i < lines.length && QUOTE.test(lines[i])) body.push(lines[i++].replace(QUOTE, ""));
      out.push({ t: "quote", c: blocks(body) });
      continue;
    }
    if (isTable(lines, i)) {
      const head = cells(line);
      const align = cells(lines[i + 1]).map((c): Align => (c.startsWith(":") && c.endsWith(":") ? "center" : c.endsWith(":") ? "right" : c.startsWith(":") ? "left" : undefined));
      const rows: Inline[][][] = [];
      i += 2;
      while (i < lines.length && !blank(lines[i]) && lines[i].includes("|")) rows.push(cells(lines[i++]).map(parseInline));
      out.push({ t: "table", align, head: head.map(parseInline), rows });
      continue;
    }
    if (ITEM.test(line)) {
      i = list(lines, i, out);
      continue;
    }
    const para: string[] = [line.trim()];
    i++;
    while (i < lines.length && !blank(lines[i]) && !startsBlock(lines, i)) para.push(lines[i++].trim());
    out.push({ t: "p", c: parseInline(para.join("\n")) });
  }
  return out;
}

/** Una lista desde `i`: sus ítems y, adentro, lo que va más sangrado */
function list(lines: string[], i: number, out: Block[]): number {
  const first = ITEM.exec(lines[i])!;
  const base = first[1].length;
  const ordered = first[3] !== undefined;
  const items: string[][] = [];
  while (i < lines.length) {
    const line = lines[i];
    const m = ITEM.exec(line);
    if (m && m[1].length <= base + 1 && (m[3] !== undefined) === ordered) {
      items.push([m[4]]);
      i++;
      continue;
    }
    if (m && m[1].length <= base + 1) break;
    const current = items[items.length - 1];
    if (blank(line)) {
      // Sigue la lista si después viene algo sangrado o otro ítem igual
      let j = i + 1;
      while (j < lines.length && blank(lines[j])) j++;
      const next = j < lines.length ? lines[j] : "";
      const nm = ITEM.exec(next);
      const continues = (next && indentOf(next) > base + 1) || (nm && nm[1].length <= base + 1 && (nm[3] !== undefined) === ordered);
      if (!continues) break;
      current.push("");
      i++;
      continue;
    }
    if (indentOf(line) > base + 1) {
      current.push(line.slice(Math.min(indentOf(line), base + 2 + (ordered ? 2 : 0))));
      i++;
      continue;
    }
    // Continuación perezosa del texto del ítem
    if (!blank(current[current.length - 1]) && !startsBlock(lines, i)) {
      current[current.length - 1] += "\n" + line.trim();
      i++;
      continue;
    }
    break;
  }
  out.push({ t: "list", ordered, start: ordered ? Number(first[3]) : 1, items: items.map(blocks) });
  return i;
}

const word = (ch: string | undefined) => !!ch && /[\p{L}\p{N}]/u.test(ch);

export function parseInline(s: string): Inline[] {
  const out: Inline[] = [];
  let buf = "";
  const flush = () => {
    if (buf) out.push({ t: "text", v: buf });
    buf = "";
  };
  let i = 0;
  while (i < s.length) {
    const ch = s[i];
    if (ch === "\\" && i + 1 < s.length && /[\\`*_[\]()#+\-.!|>~]/.test(s[i + 1])) {
      buf += s[i + 1];
      i += 2;
      continue;
    }
    if (ch === "`") {
      let n = 1;
      while (s[i + n] === "`") n++;
      const fence = "`".repeat(n);
      const end = s.indexOf(fence, i + n);
      if (end !== -1) {
        flush();
        out.push({ t: "code", v: s.slice(i + n, end).trim() || s.slice(i + n, end) });
        i = end + n;
        continue;
      }
      buf += fence;
      i += n;
      continue;
    }
    if ((ch === "*" || ch === "_") && s[i + 1] === ch && s[i + 2] && !/\s/.test(s[i + 2]) && !(ch === "_" && word(s[i - 1]))) {
      const end = s.indexOf(ch + ch, i + 2);
      if (end > i + 2 && !/\s/.test(s[end - 1])) {
        flush();
        out.push({ t: "b", c: parseInline(s.slice(i + 2, end)) });
        i = end + 2;
        continue;
      }
    }
    // Cursiva: no entre letras o números (snake_case, 2*3*4)
    if ((ch === "*" || ch === "_") && s[i + 1] !== ch && s[i + 1] && !/\s/.test(s[i + 1]) && !word(s[i - 1])) {
      let end = i + 1;
      while ((end = s.indexOf(ch, end)) !== -1 && (s[end + 1] === ch || /\s/.test(s[end - 1]) || word(s[end + 1]))) end += s[end + 1] === ch ? 2 : 1;
      if (end > i + 1) {
        flush();
        out.push({ t: "i", c: parseInline(s.slice(i + 1, end)) });
        i = end + 1;
        continue;
      }
    }
    if (ch === "[") {
      const m = /^\[([^\]]+)\]\(([^)\s]+)(?:\s+"[^"]*")?\)/.exec(s.slice(i));
      if (m) {
        flush();
        out.push({ t: "a", href: m[2], c: parseInline(m[1]) });
        i += m[0].length;
        continue;
      }
    }
    buf += ch;
    i++;
  }
  flush();
  return out;
}
