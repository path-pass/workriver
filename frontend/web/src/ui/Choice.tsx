import type { ReactNode } from 'react'
import { Select, SelectContent, SelectGroup, SelectItem, SelectLabel, SelectTrigger, SelectValue } from '@/ui/primitives/select'
import { cn } from 'cn'

/** One option of a Choice: Radix takes no empty value, so "none" is a word of its own. */
export interface Option {
  value: string
  label: string
  /** The label's language when it is content (a title), not UI text: see contentLang. */
  lang?: string
  /** Shown instead of the label when it is a UI sentence with content in it (a Slot). */
  content?: ReactNode
}

/**
 * A pick from a short list, in our colours (Radix Select: keyboard and screen readers as
 * the platform's own, a menu that is ours rather than the system's). `tone`: on the dark
 * rail, or on the page. `groups`: more options after them, each group under its heading.
 */
export function Choice({ value, onChange, options, groups = [], label, tone = 'page', placeholder, className }: {
  value: string
  onChange: (v: string) => void
  options: Option[]
  groups?: { label: string; options: Option[] }[]
  label: string
  tone?: 'rail' | 'page'
  placeholder?: string
  className?: string
}) {
  const rail = tone === 'rail'
  // The menu is portalled out of the rail, so the base item's `**:text-accent-foreground`
  // would be the page's dark ink here: override it for the item's text too.
  const item = rail
    ? 'focus:bg-rail-3 focus:text-rail-ink not-data-[variant=destructive]:focus:**:text-rail-ink'
    : 'focus:bg-sunken focus:text-ink not-data-[variant=destructive]:focus:**:text-ink'
  return (
    <Select value={value || undefined} onValueChange={onChange}>
      <SelectTrigger
        aria-label={label}
        size="sm"
        className={cn(
          'w-full min-w-0 rounded-sm text-[13px] shadow-none *:data-[slot=select-value]:truncate *:data-[slot=select-value]:block',
          rail
            ? 'border-rail-line bg-rail-2 text-rail-ink hover:bg-rail-3 focus-visible:ring-rail-ink-2/40 dark:bg-rail-2 dark:hover:bg-rail-3 [&>svg]:text-rail-ink-2'
            : 'border-line bg-surface text-ink hover:bg-sunken dark:bg-surface dark:hover:bg-sunken [&>svg]:text-ink-2',
          className,
        )}
      >
        <SelectValue placeholder={placeholder} />
      </SelectTrigger>
      <SelectContent
        position="popper"
        className={cn(
          'rounded-md text-[13px]',
          rail ? 'border border-rail-line bg-rail-2 text-rail-ink ring-0' : 'border border-line bg-surface text-ink ring-0',
        )}
      >
        {options.map(o => (
          <SelectItem key={o.value} value={o.value} className={item}>
            {o.content ?? <span lang={o.lang}>{o.label}</span>}
          </SelectItem>
        ))}
        {groups.filter(g => g.options.length).map(g => (
          <SelectGroup key={g.label} className="border-t border-line pt-1 mt-1">
            <SelectLabel className={rail ? 'text-rail-ink-2' : 'text-ink-2'}>{g.label}</SelectLabel>
            {g.options.map(o => (
              <SelectItem key={o.value} value={o.value} className={item}>
                {o.content ?? <span lang={o.lang}>{o.label}</span>}
              </SelectItem>
            ))}
          </SelectGroup>
        ))}
      </SelectContent>
    </Select>
  )
}
