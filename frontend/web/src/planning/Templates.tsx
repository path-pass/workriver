// Templates: the user's library of the steps a unit is made with, as the service gives it
// (the user's and the built-ins; the page keeps no copy). One template in full; its text
// edited in a plain textarea, the service's refusal under it with its line; a new one; one of
// the user's removed, or a built-in's name given back to the built-in. Reached from the
// rail's foot and from a unit's template name. The page makes no unit, so it picks none.
import { useEffect, useState } from 'react'
import { Button } from '@/ui/primitives/button'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/ui/primitives/dialog'
import { ScrollArea } from '@/ui/primitives/scroll-area'
import { contentLang, useT, type T } from '@/i18n'
import { removeTemplate, saveTemplate, templateText } from '@/api/actions'
import { Icon } from '@/ui/Icon'
import { cn } from '@/lib/utils'
import type { Template, TemplateStep } from './types'
import type { Workspace } from '@/workspace/types'

/** Each Templates dialog on the page, by what it opens on. */
const openers = new Set<(name?: string) => void>()
/** Opens Templates, on template `name` when given. */
export function openTemplates(name?: string) { openers.forEach(open => open(name)) }

/** "Templates" at the rail's foot (an icon when the rail is folded). */
export function TemplatesButton({ compact }: { compact?: boolean }) {
  const { t } = useT()
  return compact ? (
    <Button variant="ghost" size="icon-sm" aria-label={t('template.title')} className="rounded-sm text-rail-ink-2 hover:bg-rail-2 hover:text-rail-ink" onClick={() => openTemplates()}>
      <Icon name="template" className="size-4" />
    </Button>
  ) : (
    <Button variant="ghost" size="sm" className="justify-start gap-2 rounded-sm px-2 text-rail-ink-2 hover:bg-rail-2 hover:text-rail-ink" onClick={() => openTemplates()}>
      <Icon name="template" className="size-4" />
      {t('template.title')}
    </Button>
  )
}

/** A step's kind as the graph names it: a review by its reviewer, a gate by what it asks. */
export function stepKind(t: T, s: TemplateStep): string {
  if (s.kind === 'review') return t(`reviewer.${(s.reviewer ?? 'code-review') as 'code-review'}`)
  if (s.kind === 'gate') return t(`gate.${(s.gate ?? 'decide') as 'decide'}`)
  if (s.kind === 'tests') return t('reviewer.test')
  return t(`kind.${s.kind}`)
}

/** What a step follows: the steps named, or the unit's start. */
function afterOf(tpl: Template, k: number): string[] {
  const s = tpl.steps[k]
  if (s.after) return s.after
  return k === 0 ? [] : [tpl.steps[k - 1].id]
}

/** One template in full: what it is for, the kind of unit it is for, each step in order. */
export function TemplateView({ tpl }: { tpl: Template }) {
  const { t } = useT()
  return (
    <div className="flex flex-col gap-3 text-sm">
      <p className="text-xs text-ink-2">
        {t('template.forUnit', { unit: t(`template.unit.${tpl.unit}`) })} · {t(tpl.builtin ? 'template.builtin' : 'template.yours')}
      </p>
      <p className="whitespace-pre-line" lang={contentLang(tpl.description)}>{tpl.description}</p>
      {tpl.steps.length ? (
        <ol className="flex flex-col gap-2" aria-label={t('template.steps')}>
          {tpl.steps.map((s, k) => {
            const after = afterOf(tpl, k)
            return (
              <li key={s.id} className="flex flex-col gap-0.5 border-l-2 border-line pl-2.5">
                <span className="flex flex-wrap items-baseline gap-x-2">
                  <span className="font-mono font-semibold">{s.id}</span>
                  <span className="text-xs text-ink-2">{stepKind(t, s)}</span>
                  <span className="text-xs text-ink-2">{after.length ? t('template.after', { steps: after.join(t('list.sep')) }) : t('template.afterStart')}</span>
                </span>
                <span className="whitespace-pre-line text-xs" lang={contentLang(s.description)}>{s.description}</span>
              </li>
            )
          })}
        </ol>
      ) : <p className="text-xs text-ink-2">{t('template.noSteps')}</p>}
      <p className="text-xs text-ink-2">{t('template.changeNote')}</p>
    </div>
  )
}

/** The text of a template being edited (or a new one), saved by the service, which refuses it
 *  with the line at fault. */
function Editor({ name, onDone }: { name: string | null; onDone: (saved?: string) => void }) {
  const { t } = useT()
  const [text, setText] = useState<string | null>(name ? null : t('template.starter'))
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  useEffect(() => {
    if (!name) return
    templateText(name).then(r => setText(r.text), e => setError(t('template.loadFailed', { error: e instanceof Error ? e.message : String(e) })))
  }, [name, t])
  const save = () => {
    if (text === null) return
    setBusy(true)
    saveTemplate(text).then(
      () => { setBusy(false); onDone(/^name:\s*(\S+)/m.exec(text)?.[1]) },
      e => { setBusy(false); setError(t('template.failed', { error: e instanceof Error ? e.message : String(e) })) },
    )
  }
  return (
    <form className="flex min-h-0 flex-1 flex-col gap-2" onSubmit={e => { e.preventDefault(); save() }}>
      <textarea
        aria-label={t('template.text')}
        value={text ?? ''}
        disabled={text === null}
        onChange={e => { setText(e.target.value); setError('') }}
        spellCheck={false} autoCapitalize="off" autoCorrect="off"
        aria-invalid={!!error || undefined}
        className="min-h-64 flex-1 resize-none rounded-sm border border-line bg-surface p-2 font-mono text-xs leading-relaxed"
      />
      {error && <p role="alert" className="text-xs whitespace-pre-line wrap-anywhere text-del">{error}</p>}
      <p className="text-xs text-ink-2">{t('template.changeNote')}</p>
      <div className="flex justify-end gap-2">
        <Button type="button" variant="ghost" size="sm" onClick={() => onDone()}>{t('template.cancel')}</Button>
        <Button type="submit" size="sm" disabled={busy || text === null}>{t('template.save')}</Button>
      </div>
    </form>
  )
}

/** Removing one of the user's templates (or going back to the built-in of its name), asked first. */
function Remove({ tpl, builtinName, onDone }: { tpl: Template; builtinName: boolean; onDone: () => void }) {
  const { t } = useT()
  const [asking, setAsking] = useState(false)
  const [error, setError] = useState('')
  const label = builtinName ? t('template.back') : t('template.remove')
  if (!asking) return <Button variant="outline" size="sm" onClick={() => setAsking(true)}>{label}</Button>
  return (
    <div className="flex flex-col gap-1.5 rounded-sm border border-line p-2 text-xs" role="alertdialog" aria-label={label}>
      <p>{t(builtinName ? 'template.backAsk' : 'template.removeAsk', { name: tpl.name })}</p>
      {error && <p role="alert" className="wrap-anywhere text-del">{t('template.failed', { error })}</p>}
      <div className="flex justify-end gap-2">
        <Button type="button" variant="ghost" size="xs" onClick={() => { setAsking(false); setError('') }}>{t('template.cancel')}</Button>
        <Button type="button" size="xs" onClick={() => removeTemplate(tpl.name).then(onDone, e => setError(e instanceof Error ? e.message : String(e)))}>{label}</Button>
      </div>
    </div>
  )
}

/** The one Templates dialog of the page: the library on the left, one template on the right. */
export function TemplatesDialog({ ws }: { ws: Workspace | null }) {
  const { t } = useT()
  const [open, setOpen] = useState(false)
  const [picked, setPicked] = useState<string | null>(null)
  // Editing a template by name, or writing a new one (''); null: reading.
  const [editing, setEditing] = useState<string | null>(null)
  useEffect(() => {
    const opener = (name?: string) => { setPicked(name ?? null); setEditing(null); setOpen(true) }
    openers.add(opener)
    return () => { openers.delete(opener) }
  }, [])
  const list = ws?.templates ?? []
  const tpl = list.find(x => x.name === picked) ?? list[0]
  const builtins = new Set(['research', 'development', 'spike'])
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent closeLabel={t('settings.close')} className="flex h-[min(85vh,40rem)] flex-col sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>{t('template.title')}</DialogTitle>
          <DialogDescription>{t('template.lead')}</DialogDescription>
        </DialogHeader>
        <div className="grid min-h-0 flex-1 grid-cols-[minmax(10rem,14rem)_minmax(0,1fr)] gap-4 max-[640px]:grid-cols-1">
          <nav aria-label={t('template.list')} className="flex min-h-0 flex-col gap-1">
            <ScrollArea className="min-h-0 flex-1">
              <ul className="flex flex-col gap-0.5">
                {list.map(x => (
                  <li key={x.name}>
                    <button
                      type="button"
                      aria-current={x.name === tpl?.name || undefined}
                      onClick={() => { setPicked(x.name); setEditing(null) }}
                      className={cn('flex w-full flex-col items-start rounded-sm px-2 py-1 text-left hover:bg-sunken', x.name === tpl?.name && 'bg-sunken')}
                    >
                      <span className="font-mono text-sm">{x.name}</span>
                      <span className="text-xs text-ink-2">{t(`template.unit.${x.unit}`)} · {t(x.builtin ? 'template.builtin' : 'template.yours')}</span>
                    </button>
                  </li>
                ))}
              </ul>
              {!list.length && <p className="px-2 text-xs text-ink-2">{t('template.none')}</p>}
            </ScrollArea>
            <Button variant="outline" size="sm" className="self-start" onClick={() => setEditing('')}><Icon name="plus" className="size-3.5" />{t('template.new')}</Button>
          </nav>
          <div className="flex min-h-0 flex-col gap-3">
            {editing !== null ? (
              <Editor key={editing} name={editing || null} onDone={saved => { setEditing(null); if (saved) setPicked(saved) }} />
            ) : tpl ? (
              <>
                <h3 className="font-mono text-base font-semibold">{tpl.name}</h3>
                <ScrollArea className="min-h-0 flex-1"><TemplateView tpl={tpl} /></ScrollArea>
                <div className="flex flex-wrap justify-end gap-2">
                  {!tpl.builtin && <Remove key={tpl.name} tpl={tpl} builtinName={builtins.has(tpl.name)} onDone={() => setPicked(tpl.name)} />}
                  <Button size="sm" onClick={() => setEditing(tpl.name)}>{t('template.edit')}</Button>
                </div>
              </>
            ) : null}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}
