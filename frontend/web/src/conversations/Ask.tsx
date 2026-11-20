// The side panel's Ask section: two kinds of conversation, one box under each.
// Why: the picked task's conversations (with none picked, the unit's own); what a question is
// about is the current selection, shown as chips above the box. A temporary agent answers
// from the records and the code, and asks the agent that did the work only when it is idle,
// so an agent at work is never interrupted.
// Manager: the user's messages to a workspace's manager, typed into its terminal, and what
// it replied (`workriver reply`).
import { useEffect, useRef, useState, type DragEvent, type ReactNode, type RefObject } from 'react'
import { Button } from '@/ui/primitives/button'
import { ToggleGroup, ToggleGroupItem } from '@/ui/primitives/toggle-group'
import { askeeOf, finderLabel, findingsOf } from '@/work/select'
import { ms } from '@/planning/select'
import { baseName } from '@/code/files'
import { useT } from '@/i18n'
import { attach, detach, MAX_ATTACHMENT } from '@/api/actions'
import type { Attachment } from '@/attachments/types'
import type { Task } from '@/work/types'
import type { Unit } from '@/planning/types'
import type { AskContext, Thread, Talk } from './types'
import { Choice } from '@/ui/Choice'
import { Icon } from '@/ui/Icon'
import { Tip } from '@/ui/Tip'
import { stateOf, threadsOf, withAsked } from './select'
import { ThreadRow, ThreadBody, Chips, Log, MessageView } from './Turns'

/** What a conclusion request says to the agent; content, not UI text. */
const CONCLUDE = 'Draft a conclusion of this discussion.'

/** Whether pasted html holds text of its own (tags, styles and scripts left out). Parsed
 *  as an inert document: nothing in it runs or loads. */
function wordsIn(html: string) {
  if (!html) return false
  const doc = new DOMParser().parseFromString(html, 'text/html')
  doc.querySelectorAll('style, script').forEach(el => el.remove())
  return !!doc.body.textContent?.trim()
}

/** A drag that carries files (not text dragged into the box). */
const hasFiles = (e: DragEvent) => e.dataTransfer.types.includes('Files')

/** Conversations shown before "earlier" is opened. */
const LATEST = 3

/** The last request for the cursor that a box took, so a box shown later does not take it again. */
let tookFocus = 0

/** The box fixed at the bottom: chips and actions above it, a line under it. Enter sends, Shift+Enter breaks the line.
 *  With `onFiles` it takes files too: pasted into it (a screenshot), or picked with its button. */
function Box({ label, placeholder, focusAt, above, below, disabled, onSend, onFiles }: {
  label: string; placeholder: string; focusAt: number; above?: ReactNode; below: ReactNode; disabled?: boolean; onSend: (text: string) => void; onFiles?: (files: File[]) => void
}) {
  const { t } = useT()
  const [text, setText] = useState('')
  const box = useRef<HTMLTextAreaElement>(null)
  const picker = useRef<HTMLInputElement>(null)
  // Asked for from outside ("Ask why", `?`): the cursor comes here, once per request, and only
  // into a box on screen (the wide panel stays mounted, hidden, on a narrow screen). A frame
  // later, so a sheet opening around it first notes where focus was, and gives it back on close.
  useEffect(() => {
    if (!focusAt || focusAt === tookFocus) return
    const k = requestAnimationFrame(() => {
      const el = box.current
      if (!el || !el.getClientRects().length) return
      tookFocus = focusAt
      el.focus()
    })
    return () => cancelAnimationFrame(k)
  }, [focusAt])
  // The box grows with what is typed.
  useEffect(() => {
    const el = box.current
    if (el) { el.style.height = 'auto'; el.style.height = `${el.scrollHeight}px` }
  }, [text])
  const send = () => {
    const q = text.trim()
    if (!q || disabled) return
    onSend(q)
    setText('')
  }
  return (
    <div className="flex-none border-t border-line px-4 pt-3 pb-4 text-sm">
      {above}
      <div className="askbox">
        <textarea
          ref={box}
          rows={1}
          value={text}
          aria-label={label}
          placeholder={placeholder}
          onChange={e => setText(e.target.value)}
          onKeyDown={e => {
            if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) { e.preventDefault(); send() }
          }}
          onPaste={e => {
            // A text box first. Rich text (a spreadsheet's cells, a document's paragraph) comes
            // with a picture of itself: the text goes in, the picture is not taken. Html with
            // no text of its own (a browser's "Copy image": only an <img>) is not that.
            // Otherwise the files are taken, and text of its own beside them goes in too (not
            // when it only names them, as a file manager's copy does, or is the picture's
            // address in that <img>).
            const got = [...e.clipboardData.files]
            if (!onFiles || !got.length) return
            const html = e.clipboardData.getData('text/html')
            if (wordsIn(html)) return
            const text = e.clipboardData.getData('text/plain').trim()
            if (!text || got.some(f => f.name && text.includes(f.name)) || html.includes(text)) e.preventDefault()
            onFiles(got)
          }}
        />
        {onFiles && (
          <>
            <Tip text={t('ask.attach')}>
              <Button type="button" variant="ghost" size="icon-xs" aria-label={t('ask.attach')} onClick={() => picker.current?.click()}><Icon name="attach" className="size-3.5" /></Button>
            </Tip>
            <input ref={picker} type="file" multiple hidden onChange={e => { onFiles([...(e.target.files ?? [])]); e.target.value = '' }} />
          </>
        )}
        <Button size="icon-xs" aria-label={t('ask.send')} disabled={!text.trim() || disabled} onClick={send}><Icon name="send" className="size-3.5" /></Button>
      </div>
      <p className="mt-1.5 text-xs text-ink-2">{below}</p>
    </div>
  )
}

type Mode = 'why' | 'manager'
/** The last request to show the manager that was followed. */
let managerSeen = 0
const MODE = 'workriver.ask.mode'

/** The side panel's Ask section: why (about the picked task, or the unit) or the manager. */
export function Ask({ unit, n, finding, talk }: { unit: Unit | null; n: Task | null; finding?: string; talk: Talk | null }) {
  const { t } = useT()
  const [mode, setMode] = useState<Mode>(() => { try { return localStorage.getItem(MODE) === 'manager' ? 'manager' : 'why' } catch { return 'why' } })
  const pickMode = (m: Mode) => { setMode(m); try { localStorage.setItem(MODE, m) } catch { /* not remembered */ } }
  // Asked for from outside ("Ask why", `?`, picked lines): that is a question.
  const focusAt = talk?.focusAt ?? 0
  const seen = useRef(focusAt)
  useEffect(() => { if (focusAt !== seen.current) { seen.current = focusAt; setMode('why') } }, [focusAt])
  // Asked for from an intent's page: its manager.
  // (Kept outside the component: the section may mount with the request already made.)
  const managerAt = talk?.managerAt ?? 0
  useEffect(() => { if (managerAt > managerSeen) { managerSeen = managerAt; pickMode('manager') } }, [managerAt])
  // Why about a task, or with none picked about the unit itself (not the intent's own work).
  const about = unit && talk && (n || !unit.intentLevel)
  // Where a question can be asked, its conversations take the files dropped on the panel.
  const takeFiles = useRef<((files: File[]) => void) | null>(null)
  return (
    // A file dropped anywhere on the panel is attached to the question being written, if one
    // can be. (The browser never opens a dropped file, here or elsewhere: App's window listener.)
    <section
      aria-labelledby="side-ask" className="flex min-h-0 flex-1 flex-col" data-takes-files={about && !(mode === 'manager' && talk) ? '' : undefined}
      onDrop={e => { if (hasFiles(e)) { e.preventDefault(); takeFiles.current?.([...e.dataTransfer.files]) } }}
    >
      <div className="mb-2 flex flex-none items-center gap-2 px-4.5">
        <h2 id="side-ask" className="text-sm font-[650]">{t('ask.why')}</h2>
        <ToggleGroup
          type="single" variant="outline" size="sm" value={mode} aria-label={t('ask.mode.label')} className="ml-auto"
          onValueChange={v => { if (v) pickMode(v as Mode) }}
        >
          <ToggleGroupItem value="why" className="h-6 px-2.5 text-xs data-[state=on]:bg-sunken data-[state=on]:font-semibold">{t('ask.mode.why')}</ToggleGroupItem>
          <ToggleGroupItem value="manager" className="h-6 px-2.5 text-xs data-[state=on]:bg-sunken data-[state=on]:font-semibold"><Icon name="manager" className="size-3" />{t('ask.mode.manager')}</ToggleGroupItem>
        </ToggleGroup>
      </div>
      {mode === 'manager' && talk
        ? <ManagerTalk unit={unit} talk={talk} />
        : about
          ? <Conversations key={unit.id} unit={unit} n={n} finding={finding} talk={talk} takeFiles={takeFiles} />
          : <p className="px-4.5 text-sm text-ink-2">{t('ask.empty')}</p>}
    </section>
  )
}

function Conversations({ unit, n, finding, talk, takeFiles }: { unit: Unit; n: Task | null; finding?: string; talk: Talk; takeFiles: RefObject<((files: File[]) => void) | null> }) {
  const { t } = useT()
  const threads = n ? threadsOf(n, talk) : withAsked(unit.threads ?? [], talk.asked[''] ?? [])
  // The box continues the last open conversation, unless the user starts a new one.
  const [fresh, setFresh] = useState(false)
  const current = fresh ? undefined : threads.filter(x => !x.ended).at(-1)
  // Whom the question goes to: the task's agent; on a gate (the user's own task), the agent
  // that asked for it, or else the manager bot. It is asked only while its session is there.
  // About the unit: nobody but the temporary agent.
  const askee = n && askeeOf(unit, n)
  const live = !!askee && talk.sessions.some(x => x.agent === askee && !x.ended)
  const who = !n ? unit.title : n.kind === 'gate' ? n.title : n.taskId ? `#${n.taskId}` : finderLabel(t, n)
  const placeholder = !n ? t('ask.placeholderUnit', { unit: unit.title })
    : n.kind === 'gate' ? (askee ? t('ask.placeholderGate', { agent: askee }) : t('ask.placeholderManager'))
    : t('ask.placeholder', { who })

  // The selection, as what the question is about. A chip left out stays out until it changes.
  const f = n ? findingsOf(n).find(x => x.id === finding) : undefined
  // Picked lines say more than their file, so they take the file's place.
  const lines = talk.lines
  const chips: AskContext[] = [
    n ? { kind: 'task', label: who } : { kind: 'unit', label: who },
    ...talk.extra,
    ...(f ? [{ kind: 'finding' as const, label: f.id }] : []),
    ...(lines
      ? [{ kind: 'lines' as const, label: `${baseName(lines.path)} L${lines.from}${lines.to > lines.from ? `–${lines.to}` : ''}`, path: lines.path, from: lines.from, to: lines.to }]
      : talk.file ? [{ kind: 'file' as const, label: baseName(talk.file), path: talk.file }] : []),
  ]
  const chipKey = chips.map(c => c.label).join('|')
  const [dropped, setDropped] = useState<string[]>([])
  useEffect(() => setDropped([]), [chipKey])
  const context = chips.filter(c => !dropped.includes(c.label))

  // Files dropped on the panel, pasted into the box or picked: the page holds them, shown as
  // chips, until the question is sent (then they are stored); one taken out, or left when
  // the panel goes away, is let go and never stored. Too large for the store: refused at once.
  const [files, setFiles] = useState<Attachment[]>([])
  const [refused, setRefused] = useState<string[]>([])
  const held = useRef(files)
  held.current = files
  useEffect(() => () => held.current.forEach(detach), [])
  const addFiles = (list: File[]) => {
    setRefused(list.filter(f => f.size > MAX_ATTACHMENT).map(f => f.name))
    setFiles(fs => [...fs, ...list.filter(f => f.size <= MAX_ATTACHMENT).map(attach)])
  }
  const remove = (a: Attachment) => { detach(a); setFiles(fs => fs.filter(x => x !== a)); setRefused([]) }
  // A refusal is about the question it was made on: gone with another task picked.
  useEffect(() => setRefused([]), [n?.id])
  useEffect(() => { takeFiles.current = addFiles; return () => { takeFiles.current = null } })

  // One conversation open at a time; the one just asked in stays open. At first, one that
  // still waits for its answer, else the latest.
  const keyOf = (th: Thread, k: number) => th.id || `#${k}`
  const waiting = threads.findLastIndex(th => stateOf(th) === 'waiting')
  const first = waiting >= 0 ? waiting : threads.length - 1
  const firstKey = first >= 0 ? keyOf(threads[first], first) : null
  const [open, setOpen] = useState<string | null>(firstKey)
  const [openLast, setOpenLast] = useState(false)
  const [all, setAll] = useState(false)
  // Another piece of work: its waiting (or latest) conversation open, the rest folded.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => { setOpen(firstKey); setOpenLast(false); setAll(false); setFresh(false) }, [n?.id])
  const openKey = openLast && threads.length ? keyOf(threads[threads.length - 1], threads.length - 1) : open
  const send = (q: string) => {
    talk.onAsk(n, q, context, current?.id || null, false, files)
    setFiles([])
    setRefused([])
    setFresh(false)
    // The conversation asked in: the current one, or the new one it starts (the last).
    if (current) { setOpen(keyOf(current, threads.indexOf(current))); setOpenLast(false) } else setOpenLast(true)
  }
  const earlier = all ? 0 : Math.max(0, threads.length - LATEST)

  return (
    <>
      {/* The thread list stays compact; the open conversation takes the rest of the height.
          Relative, as the chat is: see `.chat-scroll`. */}
      <div className="relative flex min-h-0 flex-1 flex-col overflow-y-auto px-4 pb-3 text-sm">
        {earlier > 0 && (
          <button type="button" className="mb-1 flex-none self-start text-xs text-ink-2 hover:text-ink" onClick={() => setAll(true)}>{t('ask.earlier', { count: earlier })}</button>
        )}
        {!threads.length && !n && <p className="text-ink-2">{t('ask.unitHint')}</p>}
        {threads.map((th, k) => {
          if (k < earlier) return null
          const key = keyOf(th, k)
          const isOpen = openKey === key
          const body = `thread-${unit.id}-${n?.id ?? ''}-${key}`
          return (
            <section key={key} className="thread" data-open={isOpen || undefined}>
              <ThreadRow th={th} open={isOpen} body={body} onToggle={() => { setOpenLast(false); setOpen(isOpen ? null : key) }} />
              {isOpen && <ThreadBody id={body} th={th} agent={n?.agent ?? null} talk={talk} onConclude={() => talk.onAsk(n, CONCLUDE, [chips[0]], th.id, true)} />}
            </section>
          )
        })}
      </div>
      {/* Every node, and every unit, has records to answer from, so the box is never disabled. */}
      <Box
        label={t('ask.label')}
        placeholder={placeholder}
        focusAt={talk.focusAt}
        onSend={send}
        onFiles={addFiles}
        above={
          <>
            <div className="mb-1.5 flex items-start gap-2">
              <span className="min-w-0 flex-1"><Chips chips={context} onDrop={c => { setDropped(d => [...d, c.label]); setRefused([]) }} files={files} onRemove={remove} /></span>
              {current && <Button variant="ghost" size="xs" className="-my-0.5 flex-none rounded-sm" onClick={() => setFresh(true)}><Icon name="plus" className="size-3" />{t('ask.fresh')}</Button>}
            </div>
            {refused.length > 0 && (
              <p className="mb-1.5 flex items-start gap-1 text-xs text-del" role="alert">
                <span className="min-w-0 flex-1 wrap-anywhere">{t('ask.tooLarge', { names: refused.join(', '), max: MAX_ATTACHMENT / 1024 / 1024 })}</span>
                <button type="button" className="flex-none rounded-sm hover:text-ink" aria-label={t('ask.dismiss')} onClick={() => setRefused([])}><Icon name="no" className="size-3.5" /></button>
              </p>
            )}
          </>
        }
        below={!n ? t('ask.howUnit') : live ? t('ask.how', { agent: askee }) : t('ask.howAlone')}
      />
    </>
  )
}

/** How long a conversation is on screen before its new messages count as read, in ms. */
const READ_AFTER = 1500

/**
 * A conversation with a workspace's manager: the picked unit's intent's workspace, or one
 * picked from those with a manager. What the user writes is typed into the manager's
 * terminal (held while it waits for a person there); it replies with `workriver reply`.
 */
function ManagerTalk({ unit, talk }: { unit: Unit | null; talk: Talk }) {
  const { t } = useT()
  const intent = talk.intents.find(i => i.id === talk.intent) ?? (unit ? talk.intents.find(i => i.units.some(u => u.id === unit.id)) : undefined)
  const own = intent?.state === 'active' ? intent.workspace?.label : undefined
  const running = talk.sessions.filter(s => s.manager && !s.ended && s.group)
  const labels = [...new Set([own, ...running.map(s => s.group!.label), ...talk.messages.map(m => m.workspace)].filter((l): l is string => !!l))]
  const [chosen, setChosen] = useState<string | null>(talk.to ?? null)
  // Asked for again (Needs you, an intent's page): the workspace named, or the one on screen.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(() => setChosen(talk.to ?? null), [talk.managerAt])
  const label = (chosen && labels.includes(chosen) ? chosen : null) ?? own ?? labels[0] ?? null
  const manager = running.find(s => s.group!.label === label)
  const list = talk.messages.filter(m => m.workspace === label)
  // The manager's newest words not read yet: read once the conversation has been on screen a
  // moment (not in a hidden tab, nor in the panel hidden on a narrow screen).
  const newest = list.flatMap(m => [...(m.byManager && m.unread ? [m.at] : []), ...m.replies.filter(r => r.unread).map(r => r.at)]).sort((a, b) => ms(a) - ms(b)).at(-1)
  const shown = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!label || !newest) return
    let k = 0
    const look = () => {
      clearTimeout(k)
      k = window.setTimeout(() => { if (!document.hidden && shown.current?.getClientRects().length) talk.onRead(label, newest) }, READ_AFTER)
    }
    look()
    document.addEventListener('visibilitychange', look)
    return () => { clearTimeout(k); document.removeEventListener('visibilitychange', look) }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [label, newest])
  const size = list.length + ':' + list.map(m => m.step + m.replies.length).join(',')
  const options = labels.map(l => {
    const m = running.find(s => s.group!.label === l)
    return { value: l, label: m ? `${l} · ${m.agent}` : l }
  })
  return (
    <>
      <div ref={shown} className="flex min-h-0 flex-1 flex-col px-4 pb-3 text-sm">
        {labels.length > 1 && <Choice value={label ?? ''} onChange={setChosen} options={options} label={t('talk.workspace')} className="mb-2 flex-none" />}
        {label === null
          ? <p className="text-ink-2">{t('talk.noWorkspace')}</p>
          : !list.length
            ? <p className="text-ink-2">{t('talk.empty', { workspace: label })}</p>
            : (
              <Log size={size} label={t('talk.label')}>
                <ol className="chat">
                  {list.map((m, k) => (
                    <MessageView key={m.id ?? `local-${k}`} m={m} />
                  ))}
                </ol>
              </Log>
            )}
      </div>
      <Box
        label={t('talk.label')}
        placeholder={t('talk.placeholder', { agent: manager?.agent ?? t('manager.state.none') })}
        focusAt={0}
        disabled={!manager || !label}
        onSend={text => label && manager && talk.onTell(label === own && intent ? { intent: intent.id, workspace: label } : { workspace: label }, text, manager.agent)}
        below={manager ? t('talk.how', { agent: manager.agent }) : t('talk.none')}
      />
    </>
  )
}
