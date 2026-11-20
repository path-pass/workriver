// Longer text an agent or the user wrote: a note with its links, and short Markdown, folded
// when long.
import { lazy, Suspense, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/ui/primitives/collapsible'
import { contentLang, useT } from '@/i18n'
import { cn } from '@/lib/utils'
import { Icon } from '@/ui/Icon'
/** Web links in plain text, cut before trailing punctuation. */
const LINK = /https?:\/\/[^\s<>"'）。，、]+[^\s<>"'）。，、.,;:!?)]/g

/** Quiet text on the dark rail or on the page. */
const QUIET = { rail: 'text-rail-ink-2 [&_:is(a,button):hover]:text-rail-ink', page: 'text-ink-2 [&_:is(a,button):hover]:text-ink' }

/**
 * An intent's note: plain text with its line breaks, its links clickable. A long one shows
 * its first lines, with "show more".
 */
export function IntentNote({ text, tone }: { text: string; tone: 'rail' | 'page' }) {
  const { t } = useT()
  const box = useRef<HTMLParagraphElement>(null)
  const [long, setLong] = useState(false)
  const [all, setAll] = useState(false)
  // Measured while clamped: more than fits in the first lines.
  useLayoutEffect(() => {
    const el = box.current
    if (el && !all) setLong(el.scrollHeight > el.clientHeight + 1)
  }, [text, all])
  const parts: ReactNode[] = []
  let last = 0
  for (const m of text.matchAll(LINK)) {
    parts.push(text.slice(last, m.index))
    parts.push(<a key={m.index} href={m[0]} target="_blank" rel="noreferrer" className="underline underline-offset-2">{m[0]}</a>)
    last = m.index + m[0].length
  }
  parts.push(text.slice(last))
  return (
    <div className={cn('text-xs', QUIET[tone])}>
      <p ref={box} lang={contentLang(text)} className={cn('whitespace-pre-line wrap-anywhere', !all && 'line-clamp-3')}>{parts}</p>
      {(long || all) && (
        <button type="button" aria-expanded={all} className="mt-0.5 rounded-sm text-[11px] font-semibold" onClick={() => setAll(a => !a)}>
          {t(all ? 'rail.noteLess' : 'rail.noteMore')}
        </button>
      )}
    </div>
  )
}

const MarkdownView = lazy(() => import('./Markdown'))

/**
 * Longer text an agent or the user wrote (a task's input, a delivery note, a gate's note, an
 * answer): short Markdown, rendered compact by the safe renderer, which loads the first time
 * one shows; until then the plain text with its line breaks. With `fold`, a long one shows its
 * first lines, with "show more".
 */
export function Prose({ text, fold = true, onFile }: { text: string; fold?: boolean; onFile?: (path: string) => void }) {
  const { t } = useT()
  const outer = useRef<HTMLDivElement>(null)
  const inner = useRef<HTMLDivElement>(null)
  const [long, setLong] = useState(false)
  // Opened for this text: another task's text starts cut again.
  const [opened, setOpened] = useState<string | null>(null)
  const all = opened === text
  // Measured while cut, and again when the rendered Markdown replaces the plain text.
  useEffect(() => {
    const o = outer.current, i = inner.current
    if (!o || !i) return
    const check = () => setLong(i.offsetHeight > o.clientHeight + 1)
    const watch = new ResizeObserver(check)
    watch.observe(i)
    watch.observe(o)
    return () => watch.disconnect()
  }, [fold])
  const body = <Suspense fallback={<div className="whitespace-pre-line" lang={contentLang(text)}>{text}</div>}><MarkdownView text={text} compact onFile={onFile} /></Suspense>
  if (!fold) return body
  return (
    <div>
      <div ref={outer} className="prose-fold" data-cut={(!all && long) || undefined} data-all={all || undefined}><div ref={inner}>{body}</div></div>
      {(long || all) && (
        <button type="button" aria-expanded={all} className="mt-0.5 rounded-sm text-[11px] font-semibold text-ink-2 hover:text-ink" onClick={() => setOpened(all ? null : text)}>
          {t(all ? 'rail.noteLess' : 'rail.noteMore')}
        </button>
      )}
    </div>
  )
}

/** A note behind a small disclosure, where rows must stay compact (the roadmap). */
export function NoteDisclosure({ text, tone }: { text: string; tone: 'rail' | 'page' }) {
  const { t } = useT()
  return (
    <Collapsible className={QUIET[tone]}>
      <CollapsibleTrigger className="group/note mt-0.5 flex items-center gap-0.5 rounded-sm text-[11px] font-semibold">
        <Icon name="chev" className="size-3 transition-transform group-data-[state=open]/note:rotate-90" />{t('rail.note')}
      </CollapsibleTrigger>
      <CollapsibleContent className="pt-0.5"><IntentNote text={text} tone={tone} /></CollapsibleContent>
    </Collapsible>
  )
}
