// What to do first, shown in the main area while nothing is recorded yet; and starting one
// planned intent in a runtime workspace that has a manager (workriver never starts agents: a
// workspace without one gets the steps to start it).
import { useEffect, useState } from 'react'
import { Button } from '@/ui/primitives/button'
import { Dialog, DialogClose, DialogContent, DialogDescription, DialogTitle } from '@/ui/primitives/dialog'
import { useT } from '@/i18n'
import { RUN_GROUP, startIntent, type StartDelivery } from '@/api/actions'
import type { Intent } from './types'
import type { RuntimeWorkspace, Session } from '@/agents/types'
import type { Workspace } from '@/workspace/types'
import { Choice } from '@/ui/Choice'
import { Steps } from '@/commands/context'
import { Content, Slot, slot, SLOT } from '@/content/Chips'
import { Icon } from '@/ui/Icon'

/** `project`: the title of the project shown, when only it has nothing yet. */
export function Start({ project }: { project?: string }) {
  const { t } = useT()
  return (
    <section aria-labelledby="start" className="flex max-w-[70ch] flex-col gap-3 pt-2">
      <h1 id="start" className="text-xl font-[650] tracking-[-0.012em]">{t('start.title')}</h1>
      <p className="text-sm text-ink-2">{project ? <Slot text={t('start.leadProject', { project: SLOT })}><Content text={project} /></Slot> : t('start.lead')}</p>
      <Steps
        steps={[
          { text: t('start.agent') },
          { text: t('start.skill'), command: 'workriver guide manager' },
          { text: t('start.register'), command: 'workriver manager here' },
          { text: t('start.begin'), command: 'workriver backlog list' },
          { text: t('start.work') },
        ]}
      />
      <p className="text-sm text-ink-2">{t('start.trailer')} <code className="font-mono text-xs">Task: u1#3</code></p>
      <Steps steps={[{ text: t('start.import'), command: 'workriver guide import' }]} />
    </section>
  )
}

/** A runtime workspace an intent can start in, with its manager if it has one and the project it works on if known. */
interface Place extends RuntimeWorkspace { manager?: Session }

/** Every workspace (the shells' own left out): those of `project` first, then those with a manager. */
function placesOf(ws: Workspace, project: string): Place[] {
  const live = ws.sessions.filter(s => !s.ended)
  const seen: RuntimeWorkspace[] = ws.workspaces ?? live.flatMap(s => (s.group ? [s.group] : [])).filter((g, k, all) => all.findIndex(x => x.id === g.id) === k)
  return seen
    .filter(w => w.label !== RUN_GROUP && !w.linkedTo)
    .map(w => ({ ...w, manager: live.find(s => s.manager && s.group?.id === w.id) }))
    .sort((a, b) => Number(a.project !== project) - Number(b.project !== project) || Number(!a.manager) - Number(!b.manager) || a.label.localeCompare(b.label))
}

/**
 * Starting a planned intent: pick the runtime workspace (those with a manager first, with its
 * state); workriver records the start as the user's and tells that manager to split it. A
 * workspace with no manager gets the steps to start one instead.
 */
export function StartIntent({ ws, intent, onClose, onStarted }: { ws: Workspace | null; intent: Intent | null; onClose: () => void; onStarted: (intent: string, w: { id: string; label: string; manager?: string; managerState?: Intent['managerState'] }) => void }) {
  const { t } = useT()
  const project = intent?.project ?? ''
  const places = ws ? placesOf(ws, project) : []
  // Known to work on another project than the intent's: folded away, and asked about again.
  const other = (p: Place) => !!project && !!p.project && p.project !== project
  const own = places.filter(p => !other(p))
  const others = places.filter(other)
  const [picked, setPicked] = useState('')
  const [showOthers, setShowOthers] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [done, setDone] = useState<{ label: string; delivery: StartDelivery } | null>(null)
  useEffect(() => { setPicked(''); setShowOthers(false); setBusy(false); setError(''); setDone(null) }, [intent])
  const at: Place | undefined = places.find(p => p.id === picked) ?? own[0]
  const unfolded = showOthers || !own.length || (!!at && other(at))
  const titleOf = (folder: string) => ws?.projects?.find(p => p.id === folder)?.title ?? folder.split('/').pop() ?? folder
  const option = (p: Place) => {
    const label = p.machine ? `${p.label} @${p.machine}` : p.label
    const base = p.manager ? t('plan.option', { label, manager: p.manager.agent, state: t(`manager.state.${p.manager.state ?? 'unknown'}`) }) : t('plan.optionNone', { label })
    return { value: p.id, label: !p.project ? t('plan.optionUnknown', { option: base }) : other(p) ? t('plan.optionOther', { option: base, project: p.projectTitle ?? titleOf(p.project) }) : base }
  }
  const steps = (id: string) => (
    <Steps
      steps={[
        { text: t('plan.agent'), command: 'workriver guide manager' },
        { text: t('plan.register'), command: 'workriver manager here' },
        { text: t('plan.start'), command: `workriver intent start ${id}` },
      ]}
    />
  )
  const start = () => {
    if (!intent || !at?.manager) return
    setBusy(true)
    startIntent(intent.id, at.id, at.manager.agent, other(at)).then(
      delivery => { setDone({ label: at.label, delivery }); onStarted(intent.id, { id: at.id, label: at.label, manager: at.manager?.agent, managerState: at.manager?.state }) },
      e => { setBusy(false); setError(t('plan.failed', { error: String(e) })) },
    )
  }
  const said = (d: StartDelivery) => d.step === 'not-delivered'
    ? t('deliver.notDelivered', { reason: t(`deliver.reason.${d.reasonCode ?? 'unknown'}`) })
    : t('deliver.delivered', { agent: d.to ?? '' })
  return (
    <Dialog open={!!intent} onOpenChange={o => { if (!o) onClose() }}>
      <DialogContent closeLabel={t('settings.close')}>
        {intent && (
          <>
            <DialogTitle><Slot text={t('plan.title', { title: SLOT })}><Content text={intent.title} /></Slot></DialogTitle>
            {done ? (
              <>
                <DialogDescription>{t('plan.started', { label: done.label })}</DialogDescription>
                <p role="status" className={done.delivery.step === 'not-delivered' ? 'text-sm text-del' : 'text-sm text-ink-2'} title={done.delivery.reason}>{said(done.delivery)}</p>
                <div className="flex justify-end"><DialogClose asChild><Button size="sm" variant="ghost">{t('plan.close')}</Button></DialogClose></div>
              </>
            ) : !places.length ? (
              <>
                <DialogDescription>{t('plan.noWorkspace')}</DialogDescription>
                <Steps steps={[{ text: t('plan.workspace') }]} />
                {steps(intent.id)}
              </>
            ) : (
              <>
                <DialogDescription className="sr-only">{t('plan.pickLabel')}</DialogDescription>
                <label className="flex items-center gap-2 text-xs text-ink-2">
                  <span className="flex-none">{t('plan.pickLabel')}</span>
                  <Choice
                    label={t('plan.pickLabel')}
                    value={at?.id ?? ''}
                    onChange={setPicked}
                    options={own.map(option)}
                    groups={unfolded ? [{ label: t('plan.others'), options: others.map(option) }] : []}
                    placeholder={t('plan.pickOther')}
                    className="h-8 flex-1"
                  />
                </label>
                {others.length > 0 && !unfolded && (
                  <button type="button" className="justify-self-start rounded-sm text-xs text-ink-2 underline-offset-2 hover:text-ink hover:underline" onClick={() => setShowOthers(true)}>
                    {t('plan.showOthers', { count: others.length })}
                  </button>
                )}
                {!at ? null : other(at) && at.manager ? (
                  <p className="text-sm"><Slot text={t('plan.otherAsk', { manager: at.manager.agent, theirs: slot(0), mine: slot(1) })}>{[<Content key="theirs" text={at.projectTitle ?? titleOf(at.project ?? '')} />, <Content key="mine" text={titleOf(project)} />]}</Slot></p>
                ) : at.manager ? (
                  <p className="text-sm text-ink-2">{t('plan.goHint', { manager: at.manager.agent })}</p>
                ) : (
                  <>
                    <p className="text-sm text-ink-2">{t('plan.none', { label: at.label })}</p>
                    {steps(intent.id)}
                  </>
                )}
                {error && <p role="alert" className="text-xs text-del wrap-anywhere">{error}</p>}
                <div className="flex justify-end gap-2">
                  <DialogClose asChild><Button type="button" variant="ghost" size="sm">{t('rail.addCancel')}</Button></DialogClose>
                  {at?.manager && <Button type="button" size="sm" disabled={busy} onClick={start}><Icon name="play" className="size-3.5" />{other(at) ? t('plan.goOther') : t('plan.go')}</Button>}
                </div>
              </>
            )}
          </>
        )}
      </DialogContent>
    </Dialog>
  )
}
