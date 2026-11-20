// How heavy a mermaid block is, counted from its text without mermaid. Mermaid draws in a
// frame that shares the page's thread, and nothing can stop it once it started: a block over
// these bounds is not drawn at all (its code shows, under a note). The bounds are chosen by
// measurement (docs/design/ui/attachments.md): at them, the worst shapes found held the page
// for about 0.4 s, half the budget of 1 s written there (room for a slower machine).

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

/** Whether a block is too heavy to draw here. */
export function tooHeavy(code: string): boolean {
  if (code.length > BOUNDS.chars) return true
  for (const word of code.matchAll(/\S+/g)) if (word[0].length > BOUNDS.word) return true
  return weight(code) > BOUNDS.weight
}
