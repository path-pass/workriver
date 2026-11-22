// Draws a question's options as stored (surface.ts): the subset's ChoicePicker as radio
// buttons and workriver's Other last, then what the carrier puts before the action (a note,
// pictures), then the subset's Button, whose words are the page's (its Text reads `/ui/submit`, which the page fills). As
// A2UI's `required` check on it says, the Button does nothing until an option is picked.
// Nothing else is drawn, whatever the record holds. Every carrier (a decide gate, a manager's
// message) draws its options with this one component. A2UI's own renderer would replace this
// file's insides, and each carrier would then put its note after the surface and hold the
// action until a required note is there (docs/design/ui/activity.md, Choices).
import { useId, type ReactNode } from 'react'
import { Button } from '@/ui/primitives/button'
import { RadioGroup, RadioGroupItem } from '@/ui/primitives/radio-group'
import { contentLang, useT } from '@/i18n'
import { isOther, labelOf, needsNote, optionsOf, OTHER, type Choice } from './surface'
import { Content, Slot, SLOT } from '@/content/Chips'

/**
 * The options to pick from. `picked`: the value picked so far; `children`: what goes before
 * the action; `onAnswer`: the action (none: nothing to answer, the options only shown);
 * `disabled`: shown, not to be changed.
 */
export function ChoiceSurface({ choice, picked, onPick, onAnswer, disabled, children }: {
  choice: Choice; picked?: string; onPick?: (value: string) => void; onAnswer?: () => void; disabled?: boolean; children?: ReactNode
}) {
  const { t } = useT()
  const id = useId()
  const options = optionsOf(choice)
  return (
    <div className="flex flex-col gap-2">
      <RadioGroup value={picked ?? ''} onValueChange={v => onPick?.(v)} disabled={disabled} aria-label={t('choice.pick')} className="gap-1.5">
        {options.map((o, k) => (
          <label key={o.value} htmlFor={`${id}-${k}`} className="flex cursor-pointer items-start gap-2 text-sm has-disabled:cursor-default">
            <RadioGroupItem id={`${id}-${k}`} value={o.value} className="mt-0.5" />
            <span className="min-w-0 wrap-anywhere" lang={contentLang(o.label)}>{o.label}</span>
            {needsNote(choice, o.value) && <span className="flex-none text-xs text-ink-2">{t('choice.withNote')}</span>}
          </label>
        ))}
        {/* Nothing to draw: no Other either. */}
        {options.length > 0 && isOther(choice, OTHER) && (
          <label htmlFor={`${id}-other`} className="flex cursor-pointer items-start gap-2 text-sm has-disabled:cursor-default">
            <RadioGroupItem id={`${id}-other`} value={OTHER} className="mt-0.5" />
            <span className="min-w-0 wrap-anywhere">{t('choice.other')}</span>
            <span className="flex-none text-xs text-ink-2">{t('choice.otherHint')}</span>
          </label>
        )}
      </RadioGroup>
      {children}
      {onAnswer && <div><Button type="button" size="sm" className="rounded-sm" disabled={!picked} onClick={onAnswer}>{t('choice.answer')}</Button></div>}
    </div>
  )
}

/** What the user picked, by its words (Other in the page's; the value when the options no longer name it). */
export function Chose({ choice, value }: { choice?: Choice; value: string }) {
  const { t } = useT()
  const label = (choice && labelOf(choice, value)) ?? (choice && isOther(choice, value) ? t('choice.other') : value)
  return <p className="text-sm text-ink-2"><Slot text={t('choice.chose', { label: SLOT })}><span className="text-ink"><Content text={label} /></span></Slot></p>
}
