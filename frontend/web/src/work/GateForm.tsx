// What the user puts in at a waiting gate: its result (with screenshots and findings), and
// the result on its way to the agents. Nothing is sent from here: it all goes through the
// shell's actions to one place in api/actions.ts.
import { createContext, useContext, useEffect, useRef, useState, type ClipboardEvent, type DragEvent } from 'react'
import { Button } from '@/ui/primitives/button'
import { Input } from '@/ui/primitives/input'
import { ToggleGroup, ToggleGroupItem } from '@/ui/primitives/toggle-group'
import { gateKey, useAct } from './actions'
import { SEVERITIES } from './select'
import type { GateResult, NewFinding, Severity, Task } from './types'
import { useT } from '@/i18n'
import { attach, detach } from '@/api/actions'
import type { Attachment } from '@/attachments/types'
import type { Unit } from '@/planning/types'
import { Icon } from '@/ui/Icon'
import { Thumbs } from '@/attachments/Thumbs'
import { DeliveryLine } from './Findings'
import { ChoiceSurface } from '@/content/choice/Choice'
import { needsNote, noteWords, showsNothing } from '@/content/choice/surface'

/** The images among pasted or dropped files. Only images for now. */
const images = (files: FileList | File[] | null | undefined) => [...(files ?? [])].filter(f => f.type.startsWith('image/')).map(attach)

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

type Draft = { severity: Severity; problem: string; impact: string; where: string; files: Attachment[] }
const blank = (): Draft => ({ severity: 'P2', problem: '', impact: '', where: '', files: [] })

type GateDraft = {
  result: GateResult['result']
  picked?: string
  note?: string
  files: Attachment[]
  drafts: Draft[]
}
export type GateDrafts = Map<string, GateDraft>
export const GateDraftsContext = createContext<GateDrafts | null>(null)

const resultFiles = (r?: GateResult) => r ? [...r.attachments, ...r.findings.flatMap(f => f.attachments)] : []

/** A draft's pictures go unless the sent result took them over. */
export function forgetGateDraft(store: GateDrafts, key: string, retry?: GateResult, sent?: GateResult) {
  const draft = store.get(key)
  store.delete(key)
  const unsent = draft ? [...draft.files, ...draft.drafts.flatMap(d => d.files)] : []
  const seen = new Set(resultFiles(sent).map(a => a.url))
  for (const file of [...unsent, ...resultFiles(retry)]) {
    if (seen.has(file.url)) continue
    seen.add(file.url)
    detach(file)
  }
}

/**
 * A waiting gate's output: what came of it, a note, and with a problem, findings; pictures
 * welcome. A gate with options (a choice) is answered by picking one, or found a problem with.
 */
export function GateForm({ unit, gate }: { unit: Unit; gate: Task }) {
  const { t } = useT()
  const act = useAct()
  const choice = gate.input.choice
  const key = gateKey(unit, gate)
  const store = useContext(GateDraftsContext)!
  // What was sent from here and not taken by the service: the form starts from it again.
  const back = act.retry[key]
  const [form, setForm] = useState<GateDraft>(() => store.get(key) ?? {
    result: back?.result ?? (choice ? 'answered' : 'done'),
    picked: back?.selected?.[0],
    note: back?.note,
    files: back?.attachments ?? [],
    drafts: back?.findings.map(f => ({ severity: f.severity, problem: f.problem, impact: f.impact, where: f.evidence.join(', '), files: f.attachments })) ?? [],
  })
  const { result, picked, files, drafts } = form
  // A conclusion starts the note until the user edits it, including clearing it.
  const [prefill, setPrefill] = useState(gate.draft)
  useEffect(() => { if (gate.draft) setPrefill(initial => initial || gate.draft) }, [gate.draft])
  const note = form.note ?? prefill ?? ''
  const [noteMissing, setNoteMissing] = useState(false)
  const [error, setError] = useState(false)
  const update = (change: Partial<GateDraft>) => {
    const next = { ...form, ...change }
    store.set(key, next)
    setForm(next)
  }
  // A refused result still holds its pictures until it is replaced or the gate closes.
  const letGo = (a: Attachment) => { if (!resultFiles(back).some(x => x.url === a.url)) detach(a) }

  const edit = (k: number, change: Partial<Draft>) => update({ drafts: drafts.map((d, i) => (i === k ? { ...d, ...change } : d)) })
  const pasted = (add: (a: Attachment[]) => void) => (e: ClipboardEvent) => {
    const got = images(e.clipboardData.files)
    if (got.length) { e.preventDefault(); add(got) }
  }
  const dropped = (add: (a: Attachment[]) => void) => (e: DragEvent) => {
    e.preventDefault(); e.stopPropagation()
    add(images(e.dataTransfer.files))
  }
  const addNote = (a: Attachment[]) => update({ files: [...files, ...a] })
  const drop = (a: Attachment) => {
    letGo(a)
    update({ files: files.filter(x => x !== a), drafts: drafts.map(d => ({ ...d, files: d.files.filter(x => x !== a) })) })
  }
  const findings: NewFinding[] = drafts.filter(d => d.problem.trim()).map(d => ({
    severity: d.severity, problem: d.problem.trim(), impact: d.impact.trim(),
    evidence: d.where.split(/[,，\n]/).map(p => p.trim()).filter(Boolean), attachments: d.files,
  }))
  const submit = () => {
    if (result === 'problem' && !findings.length) return setError(true)
    const asked = choice && result === 'answered'
    if (asked && (!picked || (needsNote(choice, picked) && showsNothing(note)))) return setNoteMissing(true)
    const sent: GateResult = {
      result, ...(asked && { selected: [picked!] }), note: note.trim(), attachments: files,
      findings: result === 'problem' ? findings : [], at: new Date().toISOString(),
    }
    forgetGateDraft(store, key, back, sent)
    act.submitGate(unit, gate, sent)
  }
  const noteNeeded = !!(choice && result === 'answered' && picked && needsNote(choice, picked))
  // What the note is for: why, for an option that asks it; what to do instead, for Other.
  const words = choice && result === 'answered' ? noteWords(choice, picked) : undefined
  const noteBlock = (
    <>
      <textarea
        className="field" rows={2} value={note} placeholder={t(words?.prompt ?? 'gate.note')} aria-label={t(words?.prompt ?? 'gate.note')}
        aria-required={noteNeeded || undefined} aria-invalid={(noteMissing && noteNeeded) || undefined}
        onChange={e => { update({ note: e.target.value }); setNoteMissing(false) }} onPaste={pasted(addNote)}
      />
      <div className="flex flex-wrap items-center gap-2">
        <AttachButton onAdd={addNote} />
        <span className="text-xs text-ink-2">{t('attach.hint')}</span>
      </div>
      <Thumbs list={files} onRemove={drop} />
    </>
  )

  return (
    <form
      className="flex flex-col gap-2"
      onSubmit={e => { e.preventDefault(); submit() }}
      data-takes-files
      onDragOver={e => e.preventDefault()}
      onDrop={dropped(addNote)}
    >
      {/* A result sent from here that the service did not take: said, and the form is back. */}
      <DeliveryLine id={key} />
      <ToggleGroup
        type="single" variant="outline" size="sm" value={result} aria-label={t('gate.result')}
        onValueChange={v => {
          if (!v) return
          update({ result: v as GateResult['result'], drafts: v === 'problem' && !drafts.length ? [blank()] : drafts })
          setError(false)
          setNoteMissing(false)
        }}
      >
        {/* The picked one must read as picked in both themes: its own colour, not a grey tint. */}
        <ToggleGroupItem value={choice ? 'answered' : 'done'} className="h-7 px-3 text-xs data-[state=on]:border-add data-[state=on]:bg-add-row data-[state=on]:font-semibold data-[state=on]:text-add">{t(choice ? 'gate.answer' : 'gate.done')}</ToggleGroupItem>
        <ToggleGroupItem value="problem" className="h-7 px-3 text-xs data-[state=on]:border-del data-[state=on]:bg-del-row data-[state=on]:font-semibold data-[state=on]:text-del">{t('gate.problem')}</ToggleGroupItem>
      </ToggleGroup>
      {choice && result === 'answered' && (
        <ChoiceSurface choice={choice} picked={picked} onPick={v => { update({ picked: v }); setNoteMissing(false) }} onAnswer={submit}>
          {noteBlock}
          {noteMissing && <p className="text-xs text-del" role="alert">{t(words?.missing ?? 'choice.needNote')}</p>}
        </ChoiceSurface>
      )}
      {!(choice && result === 'answered') && noteBlock}
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
                  <Button type="button" variant="ghost" size="icon-xs" className="ml-auto" aria-label={t('gate.removeFinding')} onClick={() => { d.files.forEach(letGo); update({ drafts: drafts.filter((_, i) => i !== k) }) }}>
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
          <div><Button type="button" variant="outline" size="xs" className="rounded-sm" onClick={() => update({ drafts: [...drafts, blank()] })}><Icon name="plus" className="size-3" />{t('gate.addFinding')}</Button></div>
          {error && <p className="text-xs text-del" role="alert">{t('gate.needFinding')}</p>}
        </div>
      )}
      {!(choice && result === 'answered') && <div><Button type="submit" size="sm" className="rounded-sm">{t('gate.submit')}</Button></div>}
    </form>
  )
}

/** A gate the user finished in this sitting: its line on the way, and the message to copy. */
export function GateSent({ unit, gate }: { unit: Unit; gate: Task }) {
  return <DeliveryLine id={gateKey(unit, gate)} sent={gate.delivery} message />
}
