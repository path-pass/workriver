// An intent's kind, size and tags as the page shows them: the quiet chip (`Fix · S`, neutral:
// colour means the state of work, and a kind is none), its tags as chips, the rail's filter
// popover, and the edit dialog's three fields (a select, a segmented size, a token input).
import type { ReactNode } from 'react'
import { Button } from '@/ui/primitives/button'
import { Popover, PopoverContent, PopoverTrigger } from '@/ui/primitives/popover'
import { ToggleGroup, ToggleGroupItem } from '@/ui/primitives/toggle-group'
import { Choice } from '@/ui/Choice'
import { Icon } from '@/ui/Icon'
import { Tip } from '@/ui/Tip'
import { contentLang, useT } from '@/i18n'
import { cn } from '@/lib/utils'
import { filtersOn, KINDS, MAX_TAGS, SIZES, withTyped, type Filter, type Kind, type Size } from './classify'
import type { Intent } from './types'

/** Radix takes no empty value: "not set" is a word of its own. */
const NONE = 'none'

/** Its kind's label and its size, `Fix · S`; nothing when neither is set. */
export function SortChip({ intent, className }: { intent: Pick<Intent, 'kind' | 'size'>; className?: string }) {
  const { t } = useT()
  const parts = [intent.kind && t(`intent.kind.${intent.kind}`), intent.size && t(`intent.size.${intent.size}`)].filter(Boolean)
  if (!parts.length) return null
  const chip = <span className={cn('tag flex-none font-normal tracking-normal', className)}>{parts.join(' · ')}</span>
  return intent.size ? <Tip text={t(`intent.sizeHint.${intent.size}`)}>{chip}</Tip> : chip
}

/** Its tags, each a chip, as written (content, never translated). */
export function TagChips({ tags }: { tags?: string[] }) {
  if (!tags?.length) return null
  return <>{tags.map(x => <span key={x} className="tag font-normal tracking-normal" lang={contentLang(x)}>{x}</span>)}</>
}

/** One checkbox of the filter. */
function Check({ label, on, onChange, lang }: { label: string; on: boolean; onChange: (on: boolean) => void; lang?: string }) {
  return (
    <label className="flex items-start gap-2 rounded-sm px-1 py-0.5 text-[13px] leading-snug hover:bg-sunken">
      <input type="checkbox" checked={on} onChange={e => onChange(e.target.checked)} className="mt-0.5 flex-none accent-[var(--you)]" />
      <span lang={lang} className="min-w-0 wrap-anywhere">{label}</span>
    </label>
  )
}

/**
 * The rail's filter: a quiet button (the number of filters on) that opens kinds, sizes and the
 * tags of the intents shown, each a checkbox. It narrows; it never re-sorts.
 */
export function RailFilter({ filter, onChange, tags }: { filter: Filter; onChange: (f: Filter) => void; tags: string[] }) {
  const { t, num } = useT()
  const n = filtersOn(filter)
  const label = n ? t('rail.filterCount', { count: n }) : t('rail.filter')
  const toggle = <K extends keyof Filter>(k: K, v: Filter[K][number], on: boolean) =>
    onChange({ ...filter, [k]: on ? [...filter[k], v] : (filter[k] as string[]).filter(x => x !== v) })
  const group = (heading: string, body: ReactNode) => (
    <fieldset className="flex min-w-0 flex-col gap-0.5">
      <legend className="mb-0.5 text-[11px] font-semibold tracking-wide text-ink-2 uppercase">{heading}</legend>
      {body}
    </fieldset>
  )
  return (
    <Popover>
      <Tip text={label}>
        <PopoverTrigger asChild>
          <Button variant="ghost" size="icon-sm" aria-label={label} className="relative w-auto min-w-7 gap-0.5 rounded-sm px-1 text-rail-ink-2 hover:bg-rail-2 hover:text-rail-ink data-[on=true]:text-rail-ink" data-on={n > 0}>
            <Icon name="filter" />
            {n > 0 && <span aria-hidden className="text-[11px] tabular-nums">{num(n)}</span>}
          </Button>
        </PopoverTrigger>
      </Tip>
      <PopoverContent align="end" className="w-64 gap-3 border border-line bg-surface text-ink ring-0">
        {group(t('rail.filterKind'), KINDS.map(k => <Check key={k} label={t(`intent.kind.${k}`)} on={filter.kinds.includes(k)} onChange={on => toggle('kinds', k, on)} />))}
        {group(t('rail.filterSize'), SIZES.map(z => <Check key={z} label={t(`intent.sizeHint.${z}`)} on={filter.sizes.includes(z)} onChange={on => toggle('sizes', z, on)} />))}
        {group(t('rail.filterTags'), tags.length
          ? <div className="flex max-h-48 flex-col gap-0.5 overflow-y-auto">{tags.map(x => <Check key={x} label={x} lang={contentLang(x)} on={filter.tags.includes(x)} onChange={on => toggle('tags', x, on)} />)}</div>
          : <p className="px-1 text-xs text-ink-2">{t('rail.filterNoTags')}</p>)}
      </PopoverContent>
    </Popover>
  )
}

/** The kind: "Not set" first, then the four. */
export function KindSelect({ value, onChange, tone = 'page', placeholder, className }: { value: Kind | null; onChange: (k: Kind | null) => void; tone?: 'page' | 'rail'; placeholder?: string; className?: string }) {
  const { t } = useT()
  return (
    <Choice
      tone={tone}
      label={t('intent.edit.fieldKind')}
      placeholder={placeholder}
      value={value ?? (placeholder ? '' : NONE)}
      onChange={v => onChange(v === NONE ? null : (v as Kind))}
      options={[{ value: NONE, label: t('intent.notSet') }, ...KINDS.map(k => ({ value: k, label: t(`intent.kind.${k}`) }))]}
      className={className}
    />
  )
}

/** The size, a segmented control: S, M, L, Not set (what each means is read out, and in the rail's filter). */
export function SizeToggle({ value, onChange }: { value: Size | null; onChange: (z: Size | null) => void }) {
  const { t } = useT()
  return (
    <ToggleGroup type="single" variant="outline" size="sm" aria-label={t('intent.edit.fieldSize')} value={value ?? NONE} onValueChange={v => { if (v) onChange(v === NONE ? null : (v as Size)) }}>
      {SIZES.map(z => <ToggleGroupItem key={z} value={z} aria-label={t(`intent.sizeHint.${z}`)} className="h-7 px-2.5 text-xs data-[state=on]:font-semibold">{t(`intent.size.${z}`)}</ToggleGroupItem>)}
      <ToggleGroupItem value={NONE} className="h-7 px-2.5 text-xs data-[state=on]:font-semibold">{t('intent.notSet')}</ToggleGroupItem>
    </ToggleGroup>
  )
}

/**
 * Tags as chips with a field after them: Enter or a comma adds what is typed (leaving the field
 * adds it too), × or Backspace in an empty field removes one; the project's tags are offered
 * as one types. At most eight: while what is typed would pass them it stays in the field, said,
 * and none of it is added (never cut); the word goes once it fits or is cleared. The typed
 * text is the owner's (`text`), so that a save takes what is left in the field. While an input
 * method composes, its keys are its own.
 */
export function TagInput({ value, onChange, text, onText, offered, labelledBy }: { value: string[]; onChange: (tags: string[]) => void; text: string; onText: (text: string) => void; offered: string[]; labelledBy?: string }) {
  const { t } = useT()
  const over = withTyped(value, text) === null
  const add = () => {
    const next = text.trim() ? withTyped(value, text) : null
    if (!next) return
    onChange(next)
    onText('')
  }
  const full = value.length >= MAX_TAGS
  const list = 'intent-tags-offered'
  const alert = 'intent-tags-over'
  return (
    <div className="field flex min-h-8 flex-wrap items-center gap-1 py-1" role="group" aria-labelledby={labelledBy}>
      {value.map(x => (
        <span key={x} className="tag gap-0.5 pr-0.5 font-normal tracking-normal" lang={contentLang(x)}>
          {x}
          <button type="button" aria-label={t('intent.edit.tagRemove', { tag: x })} className="rounded-full p-0.5 hover:bg-line" onClick={() => onChange(value.filter(y => y !== x))}>
            <Icon name="no" className="size-3" />
          </button>
        </span>
      ))}
      <input
        className="min-w-24 flex-1 bg-transparent text-sm outline-none"
        value={text}
        disabled={full}
        list={list}
        aria-label={t('intent.edit.tagAdd')}
        placeholder={full ? '' : t('intent.edit.tagAdd')}
        aria-invalid={over || undefined}
        aria-describedby={over ? alert : undefined}
        onChange={e => onText(e.target.value)}
        onBlur={add}
        onKeyDown={e => {
          if (e.nativeEvent.isComposing) return
          if ((e.key === 'Enter' || e.key === ',') && text.trim()) { e.preventDefault(); add() }
          else if (e.key === ',') e.preventDefault()
          else if (e.key === 'Backspace' && !text && value.length) onChange(value.slice(0, -1))
        }}
      />
      <datalist id={list}>{offered.filter(x => !value.includes(x)).map(x => <option key={x} value={x} />)}</datalist>
      {over && <p id={alert} role="alert" className="basis-full text-xs text-del">{t('intent.edit.tagsOver', { max: MAX_TAGS })}</p>}
    </div>
  )
}
