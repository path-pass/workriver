// What the user puts in at a waiting gate: its result (with screenshots and findings), and
// the result on its way to the agents. Nothing is sent from here: it all goes through the
// shell's actions to one place in api/actions.ts.
import { useEffect, useRef, useState, type ClipboardEvent, type DragEvent } from 'react'
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
import { needsNote } from '@/content/choice/surface'

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

/**
 * A waiting gate's output: what came of it, a note, and with a problem, findings; pictures
 * welcome. A gate with options (a choice) is answered by picking one, or found a problem with.
 */
export function GateForm({ unit, gate }: { unit: Unit; gate: Task }) {
  const { t } = useT()
  const act = useAct()
  const choice = gate.input.choice
  const [result, setResult] = useState<GateResult['result']>(choice ? 'answered' : 'done')
  const [picked, setPicked] = useState<string>()
  const [noteMissing, setNoteMissing] = useState(false)
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
    const asked = choice && result === 'answered'
    if (asked && (!picked || (needsNote(choice, picked) && !note.trim()))) return setNoteMissing(true)
    sent.current = true
    act.submitGate(unit, gate, { result, ...(asked && { selected: [picked!] }), note: note.trim(), attachments: files, findings: result === 'problem' ? findings : [], at: new Date().toISOString() })
  }
  const noteNeeded = !!(choice && result === 'answered' && picked && needsNote(choice, picked))
  const noteBlock = (
    <>
      <textarea
        className="field" rows={2} value={note} placeholder={t(noteNeeded ? 'choice.noteNeeded' : 'gate.note')} aria-label={t(noteNeeded ? 'choice.noteNeeded' : 'gate.note')}
        aria-required={noteNeeded || undefined} aria-invalid={(noteMissing && noteNeeded) || undefined}
        onChange={e => { setNote(e.target.value); setNoteMissing(false) }} onPaste={pasted(addNote)}
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
      <ToggleGroup
        type="single" variant="outline" size="sm" value={result} aria-label={t('gate.result')}
        onValueChange={v => { if (v) { setResult(v as GateResult['result']); setError(false); setNoteMissing(false); if (v === 'problem' && !drafts.length) setDrafts([blank()]) } }}
      >
        {/* The picked one must read as picked in both themes: its own colour, not a grey tint. */}
        <ToggleGroupItem value={choice ? 'answered' : 'done'} className="h-7 px-3 text-xs data-[state=on]:border-add data-[state=on]:bg-add-row data-[state=on]:font-semibold data-[state=on]:text-add">{t(choice ? 'gate.answer' : 'gate.done')}</ToggleGroupItem>
        <ToggleGroupItem value="problem" className="h-7 px-3 text-xs data-[state=on]:border-del data-[state=on]:bg-del-row data-[state=on]:font-semibold data-[state=on]:text-del">{t('gate.problem')}</ToggleGroupItem>
      </ToggleGroup>
      {choice && result === 'answered' && (
        <ChoiceSurface choice={choice} picked={picked} onPick={v => { setPicked(v); setNoteMissing(false) }} onAnswer={submit}>
          {noteBlock}
          {noteMissing && <p className="text-xs text-del" role="alert">{t('choice.needNote')}</p>}
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
      {!(choice && result === 'answered') && <div><Button type="submit" size="sm" className="rounded-sm">{t('gate.submit')}</Button></div>}
    </form>
  )
}

/** A gate the user finished in this sitting: its line on the way, and the message to copy. */
export function GateSent({ unit, gate }: { unit: Unit; gate: Task }) {
  return <DeliveryLine id={gateKey(unit, gate)} sent={gate.delivery} message />
}
