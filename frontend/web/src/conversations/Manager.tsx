// The side panel's Manager section: the conversation with a workspace's manager. What the
// user writes is typed into the manager's terminal; it replies with `workriver reply`. What
// the user picked on screen (the unit, a task, a finding, lines, the open attachment) goes
// with the message as chips, and so do the files they drop, paste or pick; the manager
// answers itself, or asks the task's agent (`workriver ask`) and replies again.
import { useEffect, useRef, useState, type DragEvent, type ReactNode } from 'react'
import { Button } from '@/ui/primitives/button'
import { finderLabel } from '@/work/select'
import { ms } from '@/planning/select'
import { useT } from '@/i18n'
import { attach, detach, MAX_ATTACHMENT } from '@/api/actions'
import type { Attachment } from '@/attachments/types'
import type { Task } from '@/work/types'
import type { Unit } from '@/planning/types'
import type { Talk, Answer } from './types'
import { Choice } from '@/ui/Choice'
import { Icon } from '@/ui/Icon'
import { Tip } from '@/ui/Tip'
import { chipKey, chipsOf, EVERY, leaveOut, messageOf, repliesOf } from './select'
import { Chips, Log, MessageView } from './Turns'

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

/** The last request for the cursor that a box took, so a box shown later does not take it again. */
let tookFocus = 0

/** The box fixed at the bottom: chips and actions above it, a line under it. Enter sends, Shift+Enter breaks the line.
 *  With `onFiles` it takes files too: pasted into it (a screenshot), or picked with its button.
 *  (A task's discussion uses it too, under its Questions block.) */
export function Box({ label, placeholder, focusAt, above, below, disabled, closed, onSend, onFiles }: {
  label: string; placeholder: string; focusAt: number; above?: ReactNode; below: ReactNode; disabled?: boolean; closed?: boolean; onSend: (text: string) => void; onFiles?: (files: File[]) => void
}) {
  const { t } = useT()
  const [text, setText] = useState('')
  const box = useRef<HTMLTextAreaElement>(null)
  const picker = useRef<HTMLInputElement>(null)
  // Asked for from outside ("Ask the manager", `?`): the cursor comes here, once per request, and only
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
          disabled={closed}
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
            <Tip text={t('talk.attach')}>
              <Button type="button" variant="ghost" size="icon-xs" aria-label={t('talk.attach')} onClick={() => picker.current?.click()}><Icon name="attach" className="size-3.5" /></Button>
            </Tip>
            <input ref={picker} type="file" multiple hidden onChange={e => { onFiles([...(e.target.files ?? [])]); e.target.value = '' }} />
          </>
        )}
        <Button size="icon-xs" aria-label={t('talk.send')} disabled={!text.trim() || disabled} onClick={send}><Icon name="send" className="size-3.5" /></Button>
      </div>
      <p className="mt-1.5 text-xs text-ink-2">{below}</p>
    </div>
  )
}

/** How long a conversation is on screen before its new messages count as read, in ms. */
const READ_AFTER = 1500
/** The last request to show a workspace's conversation that was followed. */
let managerSeen = 0

/** The select's value for the conversation with every manager (a workspace's is `w:` and its label). */
const ALL = 'all'

/**
 * The conversation with a workspace's manager: the picked unit's intent's workspace, or one
 * picked from those with a manager, or every manager at once. What the user writes is typed
 * into the manager's terminal (held while it waits for a person there); it replies with
 * `workriver reply`. To every manager: each running one gets its copy, and replies under it.
 */
export function Manager({ unit, n, finding, talk }: { unit: Unit | null; n: Task | null; finding?: string; talk: Talk | null }) {
  const { t } = useT()
  // Files dropped anywhere on the panel go with the message being written.
  const takeFiles = useRef<((files: File[]) => void) | null>(null)
  return (
    // (The browser never opens a dropped file, here or elsewhere: App's window listener.)
    <section
      aria-labelledby="side-talk" className="flex min-h-0 flex-1 flex-col" data-takes-files={talk ? '' : undefined}
      onDrop={e => { if (hasFiles(e)) { e.preventDefault(); takeFiles.current?.([...e.dataTransfer.files]) } }}
    >
      <h2 id="side-talk" className="mb-2 flex-none px-4.5 text-sm font-[650]">{t('talk.title')}</h2>
      {talk && <ManagerTalk unit={unit} n={n} finding={finding} talk={talk} takeFiles={takeFiles} />}
    </section>
  )
}

function ManagerTalk({ unit, n, finding, talk, takeFiles }: { unit: Unit | null; n: Task | null; finding?: string; talk: Talk; takeFiles: { current: ((files: File[]) => void) | null } }) {
  const { t } = useT()
  const intent = talk.intents.find(i => i.id === talk.intent) ?? (unit ? talk.intents.find(i => i.units.some(u => u.id === unit.id)) : undefined)
  const own = intent?.state === 'active' ? intent.workspace?.label : undefined
  const running = talk.sessions.filter(s => s.manager && !s.ended && s.group)
  const labels = [...new Set([own, ...running.map(s => s.group!.label), ...talk.messages.map(m => m.workspace)].filter((l): l is string => !!l))]
  const [chosen, setChosen] = useState<string | null>(talk.to ?? null)
  // Every manager at once: offered with more than one running, or once written to; once
  // picked it stays picked, however many run (none: the draft stays and nothing is sent).
  const every = running.length > 1 || chosen === EVERY || talk.messages.some(m => m.workspace === EVERY)
  // Asked for again (Needs you, an intent's page): the workspace named, or the one on screen.
  // (Kept outside the component: the section may mount with the request already made.)
  useEffect(() => {
    if ((talk.managerAt ?? 0) > managerSeen) { managerSeen = talk.managerAt!; setChosen(talk.to ?? null) }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [talk.managerAt])
  // Asked about something on screen ("Ask the manager", `?`): the unit's own workspace.
  const focusSeen = useRef(talk.focusAt)
  useEffect(() => { if (talk.focusAt !== focusSeen.current) { focusSeen.current = talk.focusAt; setChosen(null) } }, [talk.focusAt])
  const label = (chosen !== null && (labels.includes(chosen) || (chosen === EVERY && every)) ? chosen : null) ?? own ?? labels[0] ?? null
  const all = label === EVERY
  const manager = all ? undefined : running.find(s => s.group!.label === label)
  const list = talk.messages.filter(m => m.workspace === label)
  // The options a message of the user's answered: the manager's message, or one of its replies.
  const askedOf = (a: Answer) => {
    const q = list.find(m => m.id === a.message)
    return a.reply ? q?.replies[a.reply - 1]?.choice : q?.choice
  }

  // What is picked, as chips; only to the unit's own workspace's manager. A chip left out
  // stays out until the selection changes.
  const taskLabel = n ? (n.kind === 'gate' ? n.title : n.taskId ? `#${n.taskId}` : finderLabel(t, n)) : ''
  const chips = chipsOf({ unit, n, taskLabel, finding, file: talk.file, lines: talk.lines, extra: talk.extra }, !!label && label === own)
  const allKeys = chips.map(chipKey).join('|')
  const [out, setOut] = useState<string[]>([])
  useEffect(() => setOut([]), [allKeys])
  const context = leaveOut(chips, out)

  // Files dropped on the panel, pasted into the box or picked: the page holds them, shown as
  // chips, until the message is sent (then they are stored); one taken out, or left when
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
  useEffect(() => { takeFiles.current = addFiles; return () => { takeFiles.current = null } })

  // The manager's newest words not read yet: read once the conversation has been on screen a
  // moment (not in a hidden tab, nor in the panel hidden on a narrow screen).
  const newest = list.flatMap(m => [...(m.byManager && m.unread ? [m.at] : []), ...repliesOf(m).filter(r => r.unread).map(r => r.at)]).sort((a, b) => ms(a) - ms(b)).at(-1)
  const shown = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (label === null || !newest) return
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
  const size = list.length + ':' + list.map(m => m.step + repliesOf(m).length + (m.to ?? []).map(c => c.step).join('')).join(',')
  const options = [
    ...(every ? [{ value: ALL, label: t('talk.every') }] : []),
    ...labels.map(l => {
      const m = running.find(s => s.group!.label === l)
      return { value: `w:${l}`, label: m ? `${l} · ${m.agent}` : l }
    }),
  ]
  const send = (text: string) => {
    if (label === null || (all ? !running.length : !manager)) return
    if (all) talk.onTell({ workspace: EVERY, all: true }, text, '', messageOf(context, []), files)
    else talk.onTell(label === own && intent ? { intent: intent.id, workspace: label } : { workspace: label }, text, manager!.agent, messageOf(context, []), files)
    setFiles([])
    setRefused([])
  }
  return (
    <>
      <div ref={shown} className="flex min-h-0 flex-1 flex-col px-4 pb-3 text-sm">
        {options.length > 1 && <Choice value={label === null ? '' : all ? ALL : `w:${label}`} onChange={v => setChosen(v === ALL ? EVERY : v.slice(2))} options={options} label={t('talk.workspace')} className="mb-2 flex-none" />}
        {label === null
          ? <p className="text-ink-2">{t('talk.noWorkspace')}</p>
          : !list.length
            ? <p className="text-ink-2">{all ? t('talk.everyEmpty') : t('talk.empty', { workspace: label })}</p>
            : (
              <Log size={size} label={t('talk.label')}>
                <ol className="chat">
                  {list.map((m, k) => (
                    <MessageView
                      key={m.id ?? `local-${k}`} m={m} asked={askedOf}
                      onAnswer={(a, note) => label && talk.onAnswer(label, a, note, manager?.agent ?? '')}
                    />
                  ))}
                </ol>
              </Log>
            )}
      </div>
      <Box
        label={t('talk.label')}
        placeholder={all ? t('talk.everyPlaceholder') : t('talk.placeholder', { agent: manager?.agent ?? t('manager.state.none') })}
        focusAt={talk.focusAt}
        disabled={all ? !running.length : !manager}
        onSend={send}
        onFiles={addFiles}
        above={(context.length > 0 || files.length > 0 || refused.length > 0) && (
          <>
            {(context.length > 0 || files.length > 0) && (
              <div className="mb-1.5"><Chips chips={context} onDrop={c => { setOut(o => [...o, chipKey(c)]); setRefused([]) }} files={files} onRemove={remove} /></div>
            )}
            {refused.length > 0 && (
              <p className="mb-1.5 flex items-start gap-1 text-xs text-del" role="alert">
                <span className="min-w-0 flex-1 wrap-anywhere">{t('talk.tooLarge', { names: refused.join(', '), max: MAX_ATTACHMENT / 1024 / 1024 })}</span>
                <button type="button" className="flex-none rounded-sm hover:text-ink" aria-label={t('talk.dismiss')} onClick={() => setRefused([])}><Icon name="no" className="size-3.5" /></button>
              </p>
            )}
          </>
        )}
        below={all ? (running.length ? t('talk.everyHow', { count: running.length }) : t('talk.everyNone')) : manager ? t('talk.how', { agent: manager.agent }) : t('talk.none')}
      />
    </>
  )
}
