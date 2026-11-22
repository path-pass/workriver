// A question's options as the service stores them: a fixed subset of A2UI v1.0 (a
// `createSurface` message holding a Column, a single-choice ChoicePicker and a Button with its
// Text), and the values whose answer needs a note. Core builds and checks it
// (`workriver-core/src/choices/`); the page only reads the options out of it, and ends them with
// workriver's Other. Anything that is
// not the subset's shape reads as no options at all: nothing of it is drawn.

/** The stored form: the A2UI surface, and the values that need the user's note. */
export interface Choice {
  surface: { version?: string; createSurface?: { components?: unknown[] } & Record<string, unknown> } & Record<string, unknown>
  needs_note?: string[]
}

/** One option: the value stored and sent back, the words shown (the asker's, plain text). */
export interface ChoiceOption {
  value: string
  label: string
}

/** The options, in the order shown; none when the surface is not the subset's. */
export function optionsOf(c: Choice): ChoiceOption[] {
  if (c.surface.version !== '1.0') return []
  const picker = c.surface.createSurface?.components?.[1] as { component?: unknown; options?: unknown } | undefined
  if (picker?.component !== 'ChoicePicker' || !Array.isArray(picker.options)) return []
  const options = picker.options as { label?: unknown; value?: unknown }[]
  return options.every(o => typeof o.label === 'string' && typeof o.value === 'string')
    ? options.map(o => ({ value: o.value as string, label: o.label as string }))
    : []
}

/** The words of the option with this value. */
export const labelOf = (c: Choice, value: string) => optionsOf(c).find(o => o.value === value)?.label

/**
 * The answer outside the options: every choice ends with Other, which workriver adds (core's
 * `OTHER`; the surface does not hold it). Picked with a note, the user's instruction in place
 * of an option.
 */
export const OTHER = 'other'

/** Whether this value is workriver's Other (an asker's option of that value, recorded before the rule, stays theirs). */
export const isOther = (c: Choice, value: string) => value === OTHER && labelOf(c, value) === undefined

/** Whether picking this value needs a note: an option the asker marked, and Other. */
export const needsNote = (c: Choice, value: string) => (c.needs_note ?? []).includes(value) || isOther(c, value)

/** The note's words for the value picked: what to do for Other, why for an option that needs it, else the carrier's own. */
export function noteWords(choice: Choice, picked: string | undefined) {
  if (!picked || !needsNote(choice, picked)) return undefined
  return isOther(choice, picked) ? { prompt: 'choice.otherNote', missing: 'choice.otherNeedNote' } as const : { prompt: 'choice.noteNeeded', missing: 'choice.needNote' } as const
}
