// Content in the UI's sentences: in its own language, in a sentence's slot, or as a chip
// with its name (a file, a link elsewhere).
import { Fragment, type ReactNode } from 'react'
import { contentLang } from '@/i18n'
import type { External } from '@/planning/types'
import { Tip } from '@/ui/Tip'
import { Icon } from '@/ui/Icon'
/** Content (a title, what someone typed) in its own element, marked with its guessed language. */
export const Content = ({ text }: { text: string }) => <span lang={contentLang(text)}>{text}</span>

/**
 * A UI sentence with content in it: translate with slot(i) as the i-th content's value, and
 * each content goes where its slot landed, in whatever order the language puts them, so it
 * keeps its own language inside the UI's. SLOT is slot(0), for a sentence with one.
 */
export const slot = (i: number) => String.fromCharCode(0xe000 + i)
export const SLOT = slot(0)
export function Slot({ text, children }: { text: string; children: ReactNode | ReactNode[] }) {
  const fill = Array.isArray(children) ? children : [children]
  return <>{text.split(/([\uE000-\uE0FF])/).map((part, k) => (k % 2 ? <Fragment key={k}>{fill[part.charCodeAt(0) - 0xe000]}</Fragment> : part))}</>
}

/** Where an item is tracked elsewhere (jira:ABC-12), as a small chip; a link when there is one. */
export function ExternalChip({ ext }: { ext?: External }) {
  if (!ext) return null
  const text = `${ext.system}:${ext.key}`
  return ext.url
    ? <a className="ext-chip" href={ext.url} target="_blank" rel="noreferrer noopener" onClick={e => e.stopPropagation()}><Icon name="link" className="size-3" /><ChipName name={text} /></a>
    : <span className="ext-chip"><ChipName name={text} /></span>
}

/** Longer than this, a name in a chip is shortened in the middle. */
const CHIP_CHARS = 30

/**
 * A name or path inside a chip: on one line, shortened in the middle when long so its start
 * and its extension stay ("2026-11-16-new-acc…source.md"), and cut at the end by the chip's
 * width as a last resort. The whole name is in the tooltip and read out by screen readers.
 */
export function ChipName({ name }: { name: string }) {
  if (name.length <= CHIP_CHARS) return <span className="chip-name" lang={contentLang(name)}>{name}</span>
  const ext = /\.[A-Za-z0-9]{1,8}$/.exec(name)?.[0] ?? ''
  const tail = name.slice(-Math.max(ext.length + 6, 10))
  const shown = `${name.slice(0, CHIP_CHARS - tail.length - 1)}…${tail}`
  return (
    <Tip text={<Content text={name} />}>
      <span className="chip-name" lang={contentLang(name)}>
        <span aria-hidden>{shown}</span>
        <span className="sr-only">{name}</span>
      </span>
    </Tip>
  )
}
