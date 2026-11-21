// Editing an intent's title, goal, note, kind, size and tags on its page: what Save sends, what a version
// changed, the word diff the history shows, and the service's refusal of an edit made from
// an older version (`intent.updated` with `base`; the rules are core's).
import { diffArrays } from 'diff'
import { normalTags, type Kind, type Size } from './classify'
import type { Intent, IntentText, IntentVersion } from './types'

/** What the edit dialog holds. Kind, size and tags left out: as the intent has them. */
export interface Draft {
  title: string;
  goal: string;
  note: string;
  kind?: Kind | null;
  size?: Size | null;
  tags?: string[];
}

/** The intent's text as a draft to edit. */
export const draftOf = (i: Intent | IntentText): Draft => ({ title: i.title, goal: i.goal ?? '', note: i.note ?? '', kind: i.kind ?? null, size: i.size ?? null, tags: i.tags ?? [] })

const same = (a: string[], b: string[]) => a.length === b.length && a.every((x, k) => x === b[k])

/**
 * The body of the `intent.updated` that Save records: the fields that changed against the
 * intent as the edit opened it, and that version as its base. Spaces at the ends of the
 * title and goal are not a change; the note goes as written (its indentation and blank lines
 * are Markdown). Null when nothing changed or the title is blank.
 */
export function editBody(i: Intent, d: Draft): Record<string, unknown> | null {
  const title = d.title.trim()
  if (!title) return null
  const changed: Record<string, string | string[] | null> = {}
  if (title !== i.title.trim()) changed.title = title
  if (d.goal.trim() !== (i.goal ?? '').trim()) changed.goal = d.goal.trim()
  if (d.note !== (i.note ?? '')) changed.note = d.note
  // A kind or size cleared is sent as null, tags cleared as []: left out, each stays as it is.
  if (d.kind !== undefined && d.kind !== (i.kind ?? null)) changed.kind = d.kind
  if (d.size !== undefined && d.size !== (i.size ?? null)) changed.size = d.size
  if (d.tags !== undefined && !same(normalTags(d.tags), i.tags ?? [])) changed.tags = normalTags(d.tags)
  if (!Object.keys(changed).length) return null
  return { intent: i.id, ...changed, base: i.version ?? 1 }
}

export const FIELDS = ['title', 'goal', 'note', 'kind', 'size', 'tags'] as const
export type Field = (typeof FIELDS)[number]

/** A field's value as one string to compare and to diff: tags joined, none empty. */
export const valueOf = (x: Draft | IntentText, f: Field): string => (f === 'tags' ? (x.tags ?? []).join(', ') : (x[f] ?? ''))

/**
 * Which of the fields an edit sends changed since it was opened: those whose text now is
 * not what it opened with. Each is shown before the user saves anyway.
 */
export function conflictsOf(opened: Draft, now: IntentText, body: Record<string, unknown>): Field[] {
  return FIELDS.filter(f => body[f] !== undefined && valueOf(now, f) !== valueOf(opened, f))
}

export type Change = 'created' | 'merged' | 'title' | 'goal' | 'note' | 'added' | 'kind' | 'size' | 'tags'

/** What a version changed, in the order the history says it. */
export function changesOf(v: IntentVersion): Change[] {
  if (v.how === 'created') return ['created']
  const out: Change[] = v.how === 'merged' ? ['merged'] : []
  if (v.title != null) out.push('title')
  if (v.goal != null) out.push('goal')
  if (v.note != null) out.push('note')
  if (v.added != null) out.push('added')
  // Present, even as null (cleared): the version set it.
  if (v.kind !== undefined) out.push('kind')
  if (v.size !== undefined) out.push('size')
  if (v.tags !== undefined) out.push('tags')
  return out
}

export interface Run {
  kind: 'same' | 'add' | 'del';
  text: string;
}

// Words as the language has them: Chinese and Japanese have no spaces between words.
const words = new Intl.Segmenter(undefined, { granularity: 'word' })

const tokens = (s: string) => Array.from(words.segment(s), x => x.segment)

/**
 * Two texts as runs of what stayed, was removed and was added, word by word; spaces and line
 * breaks are tokens of their own, so both texts are exactly what the runs say.
 */
export function wordDiff(before: string, after: string): Run[] {
  return diffArrays(tokens(before), tokens(after)).map(p => ({ kind: p.added ? 'add' : p.removed ? 'del' : 'same', text: p.value.join('') }))
}

/**
 * The service's refusal of an edit made from an older version, read back: which field
 * changed since and the version the intent is at now. Null for any other refusal.
 */
export function conflictOf(message: string): { field: Field; now: number } | null {
  const m = message.match(/'s (title|goal|note|kind|size|tags) changed since v\d+ \(in v\d+; it is at v(\d+)/)
  return m ? { field: m[1] as Field, now: Number(m[2]) } : null
}
