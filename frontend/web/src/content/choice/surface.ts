// A question's options as the service stores them: a fixed subset of A2UI v1.0 (a
// `createSurface` message holding a Column, a single-choice ChoicePicker and a Button with its
// Text), and the values whose answer needs a note. Core builds and checks it
// (`workriver-core/src/choices/`); the page only reads the options out of it. Anything that is
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

/** Whether picking this value needs a note saying why. */
export const needsNote = (c: Choice, value: string) => (c.needs_note ?? []).includes(value)
