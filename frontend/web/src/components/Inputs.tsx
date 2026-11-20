// What the user puts in: a waiting gate's result (with screenshots), a decision on a finding,
// and the line that follows each on its way to the agents. Nothing is sent from here: it all
// goes through the shell's actions to one place in runtime.ts.
import { useEffect, useRef, useState, type ClipboardEvent, type DragEvent } from 'react'
import { Button } from '@/ui/primitives/button'
import { Input } from '@/ui/primitives/input'
import { ToggleGroup, ToggleGroupItem } from '@/ui/primitives/toggle-group'
import { findingKey, gateKey, useAct } from '@/work/actions'
import { SEVERITIES } from '@/work/select'
import type { Decision, NewFinding } from '@/work/types'
import { contentLang, useT } from '@/i18n'
import { attach, detach, MANAGER, type DeliveryStep } from '@/api/actions'
import type { Attachment } from '@/attachments/types'
import type { Finding, Severity, Task } from '@/work/types'
import type { Unit } from '@/planning/types'
import type { Sent } from '@/conversations/types'
import { Who, ChipName } from './bits'
import { CopyButton } from '@/ui/CopyButton'
import { Icon } from '@/ui/Icon'
import { Tip } from '@/ui/Tip'
import { kindOf, Viewer } from './Viewer'

/** The images among pasted or dropped files. Only images for now. */
const images = (files: FileList | File[] | null | undefined) => [...(files ?? [])].filter(f => f.type.startsWith('image/')).map(attach)

/** Small pictures and other files as chips. A click opens the viewer over every one of them
 *  that previews (pictures, Markdown, plain text), in their order; any other file downloads.
 *  With `onRemove` (pictures still being put in a form), each has a ×. */
export function Thumbs({ list, onRemove }: { list: Attachment[]; onRemove?: (a: Attachment) => void }) {
  const { t } = useT()
  const [open, setOpen] = useState<number | null>(null)
  if (!list.length) return null
  const shown = list.filter(a => kindOf(a))
  const view = (a: Attachment) => setOpen(shown.indexOf(a))
  const files = list.filter(a => kindOf(a) !== 'image')
  const images = list.filter(a => kindOf(a) === 'image')
  return (
    <>
      {files.length > 0 && (
        <div className="mt-1 flex flex-wrap gap-1">
          {files.map(a => kindOf(a)
            ? <button key={a.id} type="button" className="ctx font-mono text-xs hover:text-ink" onClick={() => view(a)}><Icon name="file" className="size-3" /><ChipName name={a.name} /></button>
            : <a key={a.id} href={a.url} download={a.name} className="ctx font-mono text-xs"><Icon name="attach" className="size-3" /><ChipName name={a.name} /></a>)}
        </div>
      )}
      {images.length > 0 && <div className="thumbs">
        {images.map(a => (
          <span key={a.id} className="thumb">
            <button type="button" aria-label={t('attach.open', { name: a.name })} onClick={() => view(a)}><img src={a.url} alt="" /></button>
            {onRemove && <button type="button" className="x" aria-label={t('attach.remove', { name: a.name })} onClick={() => onRemove(a)}><Icon name="no" className="size-3" /></button>}
          </span>
        ))}
      </div>}
      {open !== null && shown[open] && <Viewer list={shown} at={open} onAt={setOpen} onClose={() => setOpen(null)} stored={!onRemove} />}
    </>
  )
}

/** A small button that picks images with the system's file chooser. */
function AttachButton({ onAdd }: { onAdd: (a: Attachment[]) => void }) {
  const { t } = useT()
  const input = useRef<HTMLInputElement>(null)
  return (
    <>
      <Button type="button" variant="ghost" size="xs" className="rounded-sm" onClick={() => input.current?.click()}>
        <Icon name="attach" className="size-3.5" />{t('attach.add')}
      </Button>
      <input ref={input} type="file" accept="image/*" multiple hidden onChange={e => { onAdd(images(e.target.files)); e.target.value = '' }} />
    </>
  )
}

/**
 * Where a message is: not gone yet (and can be taken back), delivered, picked up. Once the
 * service has it (`sent`), its record is shown, and its text is what gets copied.
 */
export function DeliveryLine({ id, sent, message }: { id: string; sent?: Sent; message?: boolean }) {
  const { t } = useT()
  const act = useAct()
  const local = act.delivery[id]
  const text = sent?.message ?? act.message[id]
  if (sent && (!local || local.step === 'recorded')) {
    const to = sent.to ?? ''
    const steps: [Sent['step'], string][] = sent.step === 'not-delivered'
      ? [['not-delivered', t('deliver.notDelivered', { reason: sent.reasonCode ? t(`deliver.reason.${sent.reasonCode}`) : t('deliver.reason.unknown') })]]
      : [
          ['delivered', sent.toManager ? t('deliver.toManager') : t('deliver.delivered', { agent: to })],
          ['picked', t('deliver.picked', { agent: to })],
        ]
    const at = steps.findIndex(([s]) => s === sent.step)
    return (
      <div className="mt-2 flex flex-wrap items-center gap-x-1.5 gap-y-1 text-xs" aria-live="polite">
        {steps.map(([s, label], k) => {
          const line = (
            <span key={s} className={k <= at ? (k === at ? 'font-semibold text-ink' : 'text-ink-2') : 'text-ink-2 opacity-50'}>
              {k > 0 && <span aria-hidden className="mr-1.5">→</span>}
              {s === 'picked' && k <= at && <Who name={to} />} {label}
            </span>
          )
          // The service's own words, for whoever needs the detail.
          return s === 'not-delivered' && sent.reason ? <Tip key={s} text={sent.reason}>{line}</Tip> : line
        })}
        {message && text && <CopyButton text={text} label={t('deliver.copy')} />}
      </div>
    )
  }
  const d = local
  if (!d) return null
  const manager = d.to === MANAGER
  const steps: [DeliveryStep, string][] = d.step === 'recorded' || d.step === 'failed'
    ? [['pending', t('deliver.pending')], [d.step, d.step === 'recorded' ? t('deliver.recorded') : t('deliver.failed')]]
    : [
        ['pending', t('deliver.pending')],
        ['delivered', manager ? t('deliver.toManager') : t('deliver.delivered', { agent: d.to })],
        ...(manager ? [] : [['picked', t('deliver.picked', { agent: d.to })] as [DeliveryStep, string]]),
      ]
  const at = steps.findIndex(([s]) => s === d.step)
  return (
    <div className="mt-2 flex flex-wrap items-center gap-x-1.5 gap-y-1 text-xs" aria-live="polite">
      {steps.map(([s, label], k) => (
        <span key={s} className={k <= at ? (k === at ? 'font-semibold text-ink' : 'text-ink-2') : 'text-ink-2 opacity-50'}>
          {k > 0 && <span aria-hidden className="mr-1.5">→</span>}
          {s === 'picked' && k <= at && <Who name={d.to} />} {label}
        </span>
      ))}
      {d.step === 'pending' && <Button variant="ghost" size="xs" className="rounded-sm" onClick={() => act.undo(id)}>{t('deliver.undo')}</Button>}
      {message && text && <CopyButton text={text} label={t('deliver.copy')} />}
    </div>
  )
}

/** The four decisions on an open finding; the agent's suggestion, if any, stands out. */
export function Decide({ unit, task, f }: { unit: Unit; task: Task; f: Finding }) {
  const { t } = useT()
  const act = useAct()
  const [note, setNote] = useState('')
  const decide = (d: Decision) => act.decide(unit, task, f, d, note.trim())
  return (
    <div className="mt-2 flex flex-col gap-1.5">
      <Input value={note} onChange={e => setNote(e.target.value)} placeholder={t('decide.note')} aria-label={t('decide.note')} className="h-7 text-xs md:text-xs" />
      <div role="group" aria-label={t('decide.label', { id: f.id })} className="flex flex-wrap gap-1.5">
        {(['accept', 'revise', 'reject', 'defer'] as const).map(d => {
          const suggested = f.recommendation?.decision === d
          // Revise is fixing it the user's way: the note says how, so it needs one.
          const needsNote = d === 'revise' && !note.trim()
          return (
            <Button key={d} type="button" size="xs" variant={suggested ? 'default' : 'outline'} className="rounded-sm" disabled={needsNote} aria-describedby={needsNote ? `revise-${f.id}` : undefined} onClick={() => decide(d)}>
              {suggested && <Icon name="agent" className="size-3" />}
              {t(`decide.${d}`)}
              {suggested && <span className="sr-only">{t('decide.suggested')}</span>}
            </Button>
          )
        })}
      </div>
      {!note.trim() && <p id={`revise-${f.id}`} className="text-xs text-ink-2">{t('decide.reviseNeedsNote')}</p>}
    </div>
  )
}

/** The decision taken in this sitting, on its way, with its note. */
export function Decided({ unit, task, f }: { unit: Unit; task: Task; f: Finding }) {
  const { t } = useT()
  return (
    <>
      {f.note && <div className="whitespace-pre-line"><span className="text-ink-2">{t('io.field', { name: t('decide.noteShort') })}</span><span lang={contentLang(f.note)}>{f.note}</span></div>}
      <DeliveryLine id={findingKey(unit, task, f)} sent={f.delivery} message />
    </>
  )
}

type Draft = { severity: Severity; problem: string; impact: string; where: string; files: Attachment[] }
const blank = (): Draft => ({ severity: 'P2', problem: '', impact: '', where: '', files: [] })

/** A waiting gate's output: what came of it, a note, and with a problem, findings; pictures welcome. */
export function GateForm({ unit, gate }: { unit: Unit; gate: Task }) {
  const { t } = useT()
  const act = useAct()
  const [result, setResult] = useState<'done' | 'problem'>('done')
  // A conclusion reached by asking about it starts the note; the user edits it.
  const [note, setNote] = useState(gate.draft ?? '')
  useEffect(() => { if (gate.draft) setNote(n => n || gate.draft!) }, [gate.draft])
  const [files, setFiles] = useState<Attachment[]>([])
  const [drafts, setDrafts] = useState<Draft[]>([])
  const [error, setError] = useState(false)
  // Pictures not handed in are let go when the form goes away.
  const kept = useRef<Attachment[]>([])
  kept.current = [...files, ...drafts.flatMap(d => d.files)]
  const sent = useRef(false)
  useEffect(() => () => { if (!sent.current) kept.current.forEach(detach) }, [])

  const edit = (k: number, change: Partial<Draft>) => setDrafts(ds => ds.map((d, i) => (i === k ? { ...d, ...change } : d)))
  const pasted = (add: (a: Attachment[]) => void) => (e: ClipboardEvent) => {
    const got = images(e.clipboardData.files)
    if (got.length) { e.preventDefault(); add(got) }
  }
  const dropped = (add: (a: Attachment[]) => void) => (e: DragEvent) => {
    e.preventDefault(); e.stopPropagation()
    add(images(e.dataTransfer.files))
  }
  const addNote = (a: Attachment[]) => setFiles(f => [...f, ...a])
  const drop = (a: Attachment) => { detach(a); setFiles(f => f.filter(x => x !== a)); setDrafts(ds => ds.map(d => ({ ...d, files: d.files.filter(x => x !== a) }))) }
  const findings: NewFinding[] = drafts.filter(d => d.problem.trim()).map(d => ({
    severity: d.severity, problem: d.problem.trim(), impact: d.impact.trim(),
    evidence: d.where.split(/[,，\n]/).map(p => p.trim()).filter(Boolean), attachments: d.files,
  }))
  const submit = () => {
    if (result === 'problem' && !findings.length) return setError(true)
    sent.current = true
    act.submitGate(unit, gate, { result, note: note.trim(), attachments: files, findings: result === 'problem' ? findings : [], at: new Date().toISOString() })
  }

  return (
    <form
      className="flex flex-col gap-2"
      onSubmit={e => { e.preventDefault(); submit() }}
      data-takes-files
      onDragOver={e => e.preventDefault()}
      onDrop={dropped(addNote)}
    >
      <ToggleGroup
        type="single" variant="outline" size="sm" value={result} aria-label={t('gate.result')}
        onValueChange={v => { if (v) { setResult(v as 'done' | 'problem'); setError(false); if (v === 'problem' && !drafts.length) setDrafts([blank()]) } }}
      >
        {/* The picked one must read as picked in both themes: its own colour, not a grey tint. */}
        <ToggleGroupItem value="done" className="h-7 px-3 text-xs data-[state=on]:border-add data-[state=on]:bg-add-row data-[state=on]:font-semibold data-[state=on]:text-add">{t('gate.done')}</ToggleGroupItem>
        <ToggleGroupItem value="problem" className="h-7 px-3 text-xs data-[state=on]:border-del data-[state=on]:bg-del-row data-[state=on]:font-semibold data-[state=on]:text-del">{t('gate.problem')}</ToggleGroupItem>
      </ToggleGroup>
      <textarea
        className="field" rows={2} value={note} placeholder={t('gate.note')} aria-label={t('gate.note')}
        onChange={e => setNote(e.target.value)} onPaste={pasted(addNote)}
      />
      <div className="flex flex-wrap items-center gap-2">
        <AttachButton onAdd={addNote} />
        <span className="text-xs text-ink-2">{t('attach.hint')}</span>
      </div>
      <Thumbs list={files} onRemove={drop} />
      {result === 'problem' && (
        <div className="flex flex-col gap-2">
          {drafts.map((d, k) => {
            const add = (a: Attachment[]) => edit(k, { files: [...d.files, ...a] })
            return (
              <fieldset key={k} className="draft" onDrop={dropped(add)} onDragOver={e => e.preventDefault()}>
                <legend className="sr-only">{t('gate.finding', { n: k + 1 })}</legend>
                <div className="flex items-center gap-2">
                  <ToggleGroup type="single" variant="outline" size="sm" value={d.severity} aria-label={t('gate.severity')} onValueChange={v => v && edit(k, { severity: v as Severity })}>
                    {SEVERITIES.map(s => <ToggleGroupItem key={s} value={s} className="h-6 px-2 font-mono text-xs data-[state=on]:border-you data-[state=on]:bg-you-bg data-[state=on]:font-semibold data-[state=on]:text-you-ink">{s}</ToggleGroupItem>)}
                  </ToggleGroup>
                  <Button type="button" variant="ghost" size="icon-xs" className="ml-auto" aria-label={t('gate.removeFinding')} onClick={() => { d.files.forEach(detach); setDrafts(ds => ds.filter((_, i) => i !== k)) }}>
                    <Icon name="no" className="size-3.5" />
                  </Button>
                </div>
                <Input value={d.problem} onChange={e => { edit(k, { problem: e.target.value }); setError(false) }} onPaste={pasted(add)} placeholder={t('io.problem')} aria-label={t('io.problem')} className="h-8 text-sm md:text-sm" />
                <Input value={d.impact} onChange={e => edit(k, { impact: e.target.value })} onPaste={pasted(add)} placeholder={t('io.impact')} aria-label={t('io.impact')} className="h-8 text-sm md:text-sm" />
                <textarea className="field font-mono text-xs" rows={1} value={d.where} onChange={e => edit(k, { where: e.target.value })} onPaste={pasted(add)} placeholder={t('gate.where')} aria-label={t('io.evidence')} />
                <div><AttachButton onAdd={add} /></div>
                <Thumbs list={d.files} onRemove={drop} />
              </fieldset>
            )
          })}
          <div><Button type="button" variant="outline" size="xs" className="rounded-sm" onClick={() => setDrafts(ds => [...ds, blank()])}><Icon name="plus" className="size-3" />{t('gate.addFinding')}</Button></div>
          {error && <p className="text-xs text-del" role="alert">{t('gate.needFinding')}</p>}
        </div>
      )}
      <div><Button type="submit" size="sm" className="rounded-sm">{t('gate.submit')}</Button></div>
    </form>
  )
}

/** A gate the user finished in this sitting: its line on the way, and the message to copy. */
export function GateSent({ unit, gate }: { unit: Unit; gate: Task }) {
  return <DeliveryLine id={gateKey(unit, gate)} sent={gate.delivery} message />
}
