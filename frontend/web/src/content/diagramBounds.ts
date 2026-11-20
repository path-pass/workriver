// Whether a mermaid block is drawn, from its text, without mermaid: its kind, and how heavy
// it is. Mermaid draws in a frame that shares the page's thread, and nothing can stop it once
// it started: a block of another kind or over these bounds is not drawn at all (its code
// shows, under a note). Kinds and bounds are chosen by measurement
// (docs/design/ui/attachments.md): at the bounds, the worst shapes found of each kind drawn
// held the page for about 0.4 s at most, under half the budget of 1 s written there.

/**
 * The kinds drawn, by the word a block starts with: those whose cost the weight below bounds,
 * measured. Others take numbers that make work the weight does not see (a gantt chart's
 * durations, a block diagram's `space:20000`, a packet's bit ranges), or were not measured.
 */
const KINDS: Record<string, string> = {
  graph: 'flowchart', flowchart: 'flowchart', sequenceDiagram: 'sequence', classDiagram: 'class',
  'classDiagram-v2': 'class', stateDiagram: 'state', 'stateDiagram-v2': 'state', erDiagram: 'er',
}

/**
 * A block as mermaid is given it: without its front matter and its `%%{…}%%` directives, which
 * can set any of mermaid's settings, at any depth or at the top level (where mermaid also
 * reads names it keeps nested: a top-level `rankSpacing` of 1 000 000 drew an image 2 000 142
 * px tall, u15#10 P2-1). A directive not closed runs to the end of the text, as in mermaid.
 * A front matter's title goes with it.
 */
export function withoutSettings(code: string): string {
  return code.replace(/^\s*---[ \t]*\r?\n[\s\S]*?\r?\n---[ \t]*(?:\r?\n|$)/, '').replace(/%%\{[\s\S]*?(?:\}%%|$)/g, '')
}

/** The kind of a block, when it is one drawn: the first word after front matter, directives and comments. */
export function kind(code: string): string | undefined {
  const first = withoutSettings(code).split(/\r?\n/).map(l => l.trim()).find(l => l && !l.startsWith('%%'))
  const word = first?.split(/[\s;]/)[0]
  return word && Object.hasOwn(KINDS, word) ? KINDS[word] : undefined
}

/**
 * The most a block may weigh (see `weight`), its most characters, and its longest word (a run
 * without white space: mermaid breaks one to fit in time growing with its square).
 */
export const BOUNDS = { weight: 80, chars: 10_000, word: 1_000 }

const LINK = /[<>|}{*ox]{0,2}(?:--|==|-\.|\.\.|->|~~~)[-=.>]*[>ox|{}*]{0,2}/g

/**
 * Edges and items, as a block's text gives them, statement by statement (lines, or parts
 * between `;`): its links, with `a & b --> c & d` joining every one on each side (at most a
 * quarter of the square of all); or the things it names besides a link's two ends (a block
 * diagram's row of blocks); at least one. Labels, strings and text after a `:` set apart by
 * a space (a message, a task's dates) cost little and could look like links: they are left out first.
 */
export function weight(code: string): number {
  let total = 0
  for (const raw of code.split(/[\n;]/)) {
    const s = raw.trim()
    if (!s || s.startsWith('%%')) continue
    let bare = s.replace(/"[^"]*"/g, ' ')
    for (let prev = ''; prev !== bare;) { prev = bare; bare = bare.replace(/\[[^[\]]*\]|\([^()]*\)|\{[^{}]*\}|\|[^|]*\|/g, ' ') }
    const links = (bare.match(LINK) ?? []).length
    const amps = (bare.match(/&/g) ?? []).length
    const edges = amps ? Math.ceil((amps + links + 1) ** 2 / 4) : links
    const names = bare.replace(/(?:\s:(?!:)|(?<!:):\s).*$/, ' ').replace(LINK, ' ').replace(/&/g, ' ').split(/[\s,]+/).filter(Boolean).length
    total += Math.max(1, edges, names - links - amps)
  }
  return total
}

/** Whether a block is drawn here: a kind drawn, within the bounds. */
export function drawable(code: string): boolean {
  if (code.length > BOUNDS.chars || !kind(code)) return false
  for (const word of code.matchAll(/\S+/g)) if (word[0].length > BOUNDS.word) return false
  return weight(code) <= BOUNDS.weight
}
