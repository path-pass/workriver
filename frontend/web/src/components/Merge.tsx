// Merging planned intents (a roadmap split too finely), in a dialog opened from the roadmap's
// heading: tick the intents of one project, pick the one that stays and its title. The others
// are dropped and what they said goes into its note. The user's alone, like the roadmap.
import { useState } from 'react'
import { Button } from '@/ui/primitives/button'
import { Dialog, DialogClose, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger } from '@/ui/primitives/dialog'
import { Input } from '@/ui/primitives/input'
import { useAct } from '@/work/actions'
import { contentLang, useT } from '@/i18n'
import type { Intent } from '@/planning/types'
import type { Workspace } from '@/workspace/types'
import { NoteDisclosure } from '@/content/Prose'
import { Content, slot, Slot, SLOT } from '@/content/Chips'
import { Choice } from '@/ui/Choice'

/** What an intent holds that a merge would lose; the service refuses those (`intent.merged`). */
function heldBy(ws: Workspace, i: Intent): 'tasks' | 'units' | 'findings' | 'chores' | null {
  if (i.units.some(u => u.intentLevel)) return 'tasks'
  if (i.units.length) return 'units'
  if ((ws.backlog ?? []).some(b => b.intent === i.id)) return 'findings'
  return i.chores.length ? 'chores' : null
}

/** The planned intents of each project, in roadmap order. */
function roadmaps(ws: Workspace) {
  const byProject = new Map<string, Intent[]>()
  for (const i of ws.intents.filter(i => i.state === 'planned').sort((a, b) => (a.order ?? 0) - (b.order ?? 0)))
    byProject.set(i.project ?? '', [...(byProject.get(i.project ?? '') ?? []), i])
  return byProject
}

/** "Merge…" in the roadmap's heading, for a project with two planned intents or more. */
export function MergeDialog({ ws, project }: { ws: Workspace; project: string | null }) {
  const { t, lang } = useT()
  const act = useAct()
  const maps = roadmaps(ws)
  // The project shown, or (all shown) any with something to merge, picked in the dialog.
  const eligible = [...maps.keys()].filter(p => (project === null || p === project) && maps.get(p)!.length > 1)
  const [open, setOpen] = useState(false)
  const [proj, setProj] = useState('')
  const [checked, setChecked] = useState<string[]>([])
  const [target, setTarget] = useState('')
  const [title, setTitle] = useState<string | null>(null)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  if (!eligible.length) return null

  const here = eligible.includes(proj) ? proj : eligible[0]
  const list = maps.get(here) ?? []
  const chosen = list.filter(i => checked.includes(i.id))
  const into = chosen.find(i => i.id === target) ?? chosen[0]
  const from = chosen.filter(i => i !== into)
  // The title follows the target until the user writes one of their own.
  const name = title ?? into?.title ?? ''
  // Titles quoted the language's way, each in its own language: the list is formatted with
  // markers, then each title goes where its marker landed.
  const quoted = (title: string) => <Slot text={t('rail.quoted', { title: SLOT })}><Content text={title} /></Slot>
  const titles = (l: Intent[]) => (
    <Slot text={new Intl.ListFormat(lang, { type: 'conjunction' }).format(l.map((_, k) => t('rail.quoted', { title: slot(k) })))}>
      {l.map(i => <Content key={i.id} text={i.title} />)}
    </Slot>
  )
  const ready = !!into && from.length > 0 && !!name.trim() && !busy
  const reset = () => { setChecked([]); setTarget(''); setTitle(null); setError(''); setBusy(false) }
  const merge = () => {
    if (!ready) return
    setBusy(true)
    setError('')
    const renamed = name.trim() !== into.title ? name.trim() : undefined
    act.merge(into.id, from.map(i => i.id), renamed).then(
      () => { setOpen(false); reset() },
      e => { setBusy(false); setError(t('rail.failed', { error: String(e) })) },
    )
  }
  const projectTitle = (p: string) => ws.projects?.find(x => x.id === p)?.title ?? p

  return (
    <Dialog open={open} onOpenChange={o => { setOpen(o); if (o) reset() }}>
      <DialogTrigger asChild>
        <Button variant="ghost" size="sm" className="h-5 rounded-sm px-1.5 text-[11px] font-semibold tracking-normal normal-case text-rail-ink-2 hover:bg-rail-2 hover:text-rail-ink">
          {t('rail.mergeOpen')}
        </Button>
      </DialogTrigger>
      <DialogContent closeLabel={t('settings.close')} className="max-h-[85vh] overflow-y-auto sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>{t('rail.mergeTitle')}</DialogTitle>
          <DialogDescription className="text-xs text-ink-2">{t('rail.mergeHint')}</DialogDescription>
        </DialogHeader>
        <form className="flex min-w-0 flex-col gap-3" onSubmit={e => { e.preventDefault(); merge() }}>
          {eligible.length > 1 && (
            <label className="flex items-center gap-2 text-xs text-ink-2">
              {t('rail.project')}
              <Choice label={t('rail.project')} value={here} onChange={p => { setProj(p); setChecked([]); setTarget(''); setTitle(null) }} options={eligible.map(p => ({ value: p, label: projectTitle(p), lang: contentLang(projectTitle(p)) }))} className="h-7 flex-1" />
            </label>
          )}
          <ul className="flex flex-col gap-0.5" aria-label={t('rail.roadmap')}>
            {list.map(i => {
              const held = heldBy(ws, i)
              return (
                <li key={i.id} className="flex items-start gap-2 rounded-md px-2 py-1.5 hover:bg-sunken" data-held={held || undefined}>
                  <input
                    type="checkbox"
                    id={`merge-${i.id}`}
                    className="mt-1 flex-none accent-[var(--you)]"
                    disabled={!!held}
                    checked={checked.includes(i.id)}
                    onChange={e => setChecked(c => (e.target.checked ? [...c, i.id] : c.filter(x => x !== i.id)))}
                  />
                  <div className="min-w-0 flex-1">
                    <label htmlFor={`merge-${i.id}`} className={held ? 'text-ink-2' : ''}>
                      <span className="block font-[550] wrap-anywhere" lang={contentLang(i.title)}>{i.title}</span>
                      {i.goal && <span className="block truncate text-xs text-ink-2" lang={contentLang(i.goal)}>{i.goal}</span>}
                    </label>
                    {held && <span className="block text-xs text-ink-2">{t(`rail.held.${held}`)}</span>}
                    {i.note?.trim() && <NoteDisclosure text={i.note.trim()} tone="page" />}
                  </div>
                </li>
              )
            })}
          </ul>
          {into && from.length > 0 && (
            <div className="flex flex-col gap-2 border-t border-line pt-3">
              <label className="flex items-center gap-2 text-xs text-ink-2">
                <span className="w-28 flex-none">{t('rail.mergeInto')}</span>
                <Choice label={t('rail.mergeInto')} value={into.id} onChange={setTarget} options={chosen.map(i => ({ value: i.id, label: i.title, lang: contentLang(i.title) }))} className="h-7 flex-1" />
              </label>
              <label className="flex items-center gap-2 text-xs text-ink-2">
                <span className="w-28 flex-none">{t('rail.mergeNewTitle')}</span>
                <Input value={name} lang={contentLang(name)} onChange={e => setTitle(e.target.value)} aria-invalid={!name.trim() || undefined} className="h-7 flex-1 text-[13px] md:text-[13px]" />
              </label>
              <p className="text-xs text-ink-2">
                {/* The titles are content: each keeps its own language inside the sentence. */}
                <Slot text={t('rail.mergeWhat', { from: slot(0), into: slot(1) })}>
                  {[<span key="from">{titles(from)}</span>, <span key="into">{quoted(name.trim() || into.title)}</span>]}
                </Slot>
                {name.trim() && name.trim() !== into.title && <> <Slot text={t('rail.mergeRename', { into: slot(0), title: slot(1) })}>{[<span key="into" lang={contentLang(into.title)}>{into.title}</span>, <span key="title" lang={contentLang(name.trim())}>{name.trim()}</span>]}</Slot></>}
              </p>
            </div>
          )}
          {error && <p role="alert" className="text-xs text-del">{error}</p>}
          <div className="flex justify-end gap-2">
            <DialogClose asChild><Button type="button" variant="ghost" size="sm">{t('rail.addCancel')}</Button></DialogClose>
            <Button type="submit" size="sm" disabled={!ready}>{t('rail.merge')}</Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  )
}
