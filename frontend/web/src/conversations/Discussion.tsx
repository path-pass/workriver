// The box under a task's Questions block where the user discusses a delivered design or
// research with the agent that did it: each question goes straight to that agent (held while
// it works), never to a temporary one; "Draft conclusion" asks it for the conclusion, after it
// amends its delivery if the result changed. Disabled, saying why, once its agent is gone or
// the gate that asks about the result is submitted; the manager (the detail's own button) is
// the way on from there.
import { useEffect, useRef, useState, type DragEvent } from 'react'
import { Button } from '@/ui/primitives/button'
import { useT } from '@/i18n'
import { attach, detach, MAX_ATTACHMENT } from '@/api/actions'
import type { Attachment } from '@/attachments/types'
import type { Task } from '@/work/types'
import type { Unit } from '@/planning/types'
import type { Talk } from './types'
import { Box } from './Manager'
import { Chips } from './Turns'

/** A drag that carries files (not text dragged into the box). */
const hasFiles = (e: DragEvent) => e.dataTransfer.types.includes('Files')

export function Discussion({ unit, n, talk }: { unit: Unit; n: Task; talk: Talk }) {
  const { t } = useT()
  const agent = n.agent ?? ''
  const state = n.discuss ?? 'open'
  const gate = unit.tasks.find(x => x.id === n.output.gate)
  // The user's files, held by the page until the question is sent (as in the Manager box).
  const [files, setFiles] = useState<Attachment[]>([])
  const [refused, setRefused] = useState<string[]>([])
  const [error, setError] = useState('')
  const held = useRef(files)
  held.current = files
  useEffect(() => () => held.current.forEach(detach), [])
  const addFiles = (list: File[]) => {
    setRefused(list.filter(f => f.size > MAX_ATTACHMENT).map(f => f.name))
    setFiles(fs => [...fs, ...list.filter(f => f.size <= MAX_ATTACHMENT).map(attach)])
  }
  const remove = (a: Attachment) => { detach(a); setFiles(fs => fs.filter(x => x !== a)); setRefused([]) }
  const send = (text: string, conclude = false) => {
    setError('')
    const sent = files
    setFiles([])
    setRefused([])
    talk.onDiscuss(unit.id, n.id, agent, text, conclude, sent).catch((e: Error) => setError(e.message))
  }
  const closed = state !== 'open'
  // A conclusion is of a discussion: offered once there is one (the service refuses it before).
  const discussed = (n.threads ?? []).some(th => th.discuss)
  const why = state === 'gone' ? t('discuss.gone', { agent }) : state === 'over' ? t('discuss.over', { gate: gate ? `#${gate.taskId ?? gate.id}` : '' }) : t('discuss.how', { agent })
  return (
    <div
      className="-mx-3 mt-2 -mb-3" data-takes-files={closed ? undefined : ''}
      onDrop={e => { if (!closed && hasFiles(e)) { e.preventDefault(); addFiles([...e.dataTransfer.files]) } }}
    >
      <Box
        label={t('discuss.label', { agent })}
        placeholder={t('discuss.placeholder', { agent })}
        focusAt={0}
        disabled={closed}
        closed={closed}
        onSend={text => send(text)}
        onFiles={closed ? undefined : addFiles}
        above={
          <>
            {files.length > 0 && <div className="mb-1.5"><Chips chips={[]} files={files} onRemove={remove} /></div>}
            {refused.length > 0 && <p className="mb-1.5 text-xs text-del" role="alert">{t('talk.tooLarge', { names: refused.join(', '), max: MAX_ATTACHMENT / 1024 / 1024 })}</p>}
            {error && <p className="mb-1.5 text-xs wrap-anywhere text-del" role="alert">{t('discuss.failed', { error })}</p>}
          </>
        }
        below={
          <span className="flex flex-wrap items-center gap-2">
            <span className="min-w-0 flex-1">{why}</span>
            {/* Closed: the way on is the manager, whose button is under the detail already. */}
            {!closed && discussed && <Button variant="outline" size="xs" className="rounded-sm" onClick={() => send('', true)}>{t('discuss.conclude')}</Button>}
          </span>
        }
      />
    </div>
  )
}
