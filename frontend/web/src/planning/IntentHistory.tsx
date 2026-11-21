// An intent's history on its page: a quiet line under its goal once it has more than one
// version ("v5 · edited 2d ago by claude 1 · History"), and the dialog it opens: the
// versions, the newest first, and the one picked against the one before it, word by word,
// or whole; an older one can be restored, which opens the edit dialog filled with it.
import { useEffect, useState } from 'react'
import { Button } from '@/ui/primitives/button'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/ui/primitives/dialog'
import { contentLang, useT } from '@/i18n'
import { Prose } from '@/content/Prose'
import { Icon } from '@/ui/Icon'
import { Tip } from '@/ui/Tip'
import { intentHistory, intentVersion } from '@/api/actions'
import { changesOf, valueOf, wordDiff, type Run } from './edit'
import { SortChip, TagChips } from './Sort'
import type { Intent, IntentText, IntentVersion } from './types'

/** Runs of a word diff: what was removed struck through in red, what was added in green. */
export function Runs({ runs }: { runs: Run[] }) {
  return (
    <>
      {runs.map((r, k) =>
        r.kind === 'add' ? <ins key={k} className="rounded-[3px] bg-add-row text-add no-underline">{r.text}</ins>
        : r.kind === 'del' ? <del key={k} className="rounded-[3px] bg-del-row text-del">{r.text}</del>
        : <span key={k}>{r.text}</span>,
      )}
    </>
  )
}

/** Who made a version: "you", or the agent with its mark. */
function Who({ name }: { name: string | null }) {
  const { t } = useT()
  if (!name) return <span>{t('intent.history.you')}</span>
  return <span className="inline-flex items-center gap-1"><Icon name="agent" className="size-3" />{name}</span>
}

/** The quiet line under the goal, with "History"; nothing for an intent never changed. */
export function HistoryLine({ intent, onOpen }: { intent: Intent; onOpen: () => void }) {
  const { t, waited, when } = useT()
  if ((intent.version ?? 1) < 2 || !intent.edited) return null
  const at = Date.parse(intent.edited.at)
  return (
    <p className="mt-1 flex flex-wrap items-center gap-x-1.5 text-xs text-ink-2">
      <Tip text={when(at)}>
        <span>{t('intent.history.line', { n: intent.version, time: waited(at), who: intent.edited.by ?? t('intent.history.you') })}</span>
      </Tip>
      <span aria-hidden>·</span>
      <button type="button" onClick={onOpen} className="rounded-sm font-semibold text-ink-2 underline-offset-2 hover:text-ink hover:underline">{t('intent.history.open')}</button>
    </p>
  )
}

/** What a version changed, in words: "title, note", "added to the note", "created". */
function What({ v }: { v: IntentVersion }) {
  const { t, lang } = useT()
  const words = changesOf(v).map(c => (c === 'merged' ? t('intent.history.merged', { from: (v.merged ?? []).join(', ') }) : t(`intent.history.${c}`)))
  return <>{new Intl.ListFormat(lang, { type: 'conjunction' }).format(words)}</>
}

/** One field of the picked version against the one before it; nothing when it did not change. */
function Field({ label, before, now, prose }: { label: string; before: string; now: string; prose?: boolean }) {
  if (before === now) return null
  return (
    <div>
      <h3 className="mb-0.5 text-xs font-semibold text-ink-2">{label}</h3>
      <p className={`wrap-anywhere ${prose ? 'max-h-[45vh] overflow-y-auto whitespace-pre-wrap rounded-lg border border-line px-2.5 py-1.5' : ''} text-sm`} lang={contentLang(now)}>
        <Runs runs={wordDiff(before, now)} />
      </p>
    </div>
  )
}

export function HistoryDialog({ intent, onClose, onRestore }: {
  intent: Intent
  onClose: () => void
  /** Opens the edit dialog with version `n`'s text. */
  onRestore: (text: IntentText, n: number) => void
}) {
  const { t, waited, when } = useT()
  const [versions, setVersions] = useState<IntentVersion[] | null>(null)
  const [picked, setPicked] = useState<number | null>(null)
  const [shown, setShown] = useState<{ n: number; text: IntentText; before: IntentText | null } | null>(null)
  const [whole, setWhole] = useState(false)
  const [error, setError] = useState('')
  const newest = versions?.[versions.length - 1]?.n ?? 0
  const n = picked ?? newest

  useEffect(() => {
    intentHistory(intent).then(setVersions, e => setError(t('intent.history.failed', { error: String(e instanceof Error ? e.message : e) })))
  }, [intent, t])
  useEffect(() => {
    if (!n) return
    let current = true
    intentVersion(intent, n).then(
      v => { if (current) setShown({ n, ...v }) },
      e => { if (current) setError(t('intent.history.failed', { error: String(e instanceof Error ? e.message : e) })) },
    )
    return () => { current = false }
  }, [intent, n, t])

  const detail = shown?.n === n ? shown : null
  // Kind and size in words ("Not set" when none), to set one version against another.
  const kindWords = (x: IntentText) => (x.kind ? t(`intent.kind.${x.kind}`) : t('intent.notSet'))
  const sizeWords = (x: IntentText) => (x.size ? t(`intent.size.${x.size}`) : t('intent.notSet'))
  const first = detail && !detail.before
  return (
    <Dialog open onOpenChange={o => { if (!o) onClose() }}>
      <DialogContent closeLabel={t('settings.close')} className="max-h-[90vh] overflow-y-auto sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>{t('intent.history.heading')} <span className="font-mono text-sm font-normal text-ink-2">{intent.id}</span></DialogTitle>
          <DialogDescription className="text-xs text-ink-2">{t('intent.history.hint')}</DialogDescription>
        </DialogHeader>
        {error && <p role="alert" className="text-xs text-del wrap-anywhere">{error}</p>}
        <div className="flex min-w-0 flex-col gap-3 sm:flex-row">
          <ol className="flex flex-none flex-col gap-0.5 sm:w-56" aria-label={t('intent.history.heading')}>
            {[...(versions ?? [])].reverse().map(v => {
              const at = Date.parse(v.at)
              return (
                <li key={v.n}>
                  <button type="button" aria-pressed={v.n === n} onClick={() => { setPicked(v.n); setWhole(false) }} className="flex w-full flex-col items-start rounded-md px-2 py-1.5 text-left text-xs hover:bg-sunken aria-pressed:bg-you-bg aria-pressed:text-you-ink">
                    <span className="flex w-full items-baseline gap-1.5">
                      <span className="font-mono">v{v.n}</span>
                      <span className="min-w-0 flex-1 truncate font-[550]"><Who name={v.by === 'user' ? null : (v.name ?? v.by)} /></span>
                      <Tip text={when(at)}><span className="flex-none tabular-nums text-ink-2">{waited(at)}</span></Tip>
                    </span>
                    <span className="text-ink-2"><What v={v} /></span>
                  </button>
                </li>
              )
            })}
          </ol>
          <div className="flex min-w-0 flex-1 flex-col gap-3 sm:border-l sm:border-line sm:pl-3">
            {detail && (
              <>
                {first || whole ? (
                  <div className="flex flex-col gap-2">
                    {first && <p className="text-xs text-ink-2">{t('intent.history.first')}</p>}
                    <p className="text-sm font-[650] wrap-anywhere" lang={contentLang(detail.text.title)}>{detail.text.title}</p>
                    {detail.text.goal && <p className="text-sm text-ink-2 wrap-anywhere" lang={contentLang(detail.text.goal)}>{detail.text.goal}</p>}
                    {(detail.text.kind || detail.text.size || !!detail.text.tags?.length) && <p className="flex flex-wrap items-center gap-1"><SortChip intent={{ kind: detail.text.kind ?? undefined, size: detail.text.size ?? undefined }} /><TagChips tags={detail.text.tags} /></p>}
                    {detail.text.note.trim() && <div className="max-h-[45vh] overflow-y-auto text-sm"><Prose text={detail.text.note} fold={false} /></div>}
                  </div>
                ) : (
                  <>
                    <Field label={t('intent.edit.fieldTitle')} before={detail.before!.title} now={detail.text.title} />
                    <Field label={t('intent.edit.fieldGoal')} before={detail.before!.goal} now={detail.text.goal} />
                    <Field label={t('intent.edit.fieldNote')} before={detail.before!.note} now={detail.text.note} prose />
                    <Field label={t('intent.edit.fieldKind')} before={kindWords(detail.before!)} now={kindWords(detail.text)} />
                    <Field label={t('intent.edit.fieldSize')} before={sizeWords(detail.before!)} now={sizeWords(detail.text)} />
                    <Field label={t('intent.edit.fieldTags')} before={valueOf(detail.before!, 'tags')} now={valueOf(detail.text, 'tags')} />
                  </>
                )}
                <div className="flex flex-wrap items-center justify-end gap-2">
                  {!first && <Button type="button" variant="ghost" size="sm" aria-pressed={whole} onClick={() => setWhole(w => !w)}>{t('intent.history.whole')}</Button>}
                  {n !== newest && <Button type="button" variant="outline" size="sm" onClick={() => onRestore(detail.text, n)}>{t('intent.history.restore')}</Button>}
                </div>
              </>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}
