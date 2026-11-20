// A task's discussion in its Questions block: shown and open however many newer
// conversations there are, read only while it is on screen; and "Draft conclusion" only once
// there is something to conclude.
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { Questions } from './Turns'
import { Discussion } from './Discussion'
import type { Talk, Thread, Turn } from './types'
import type { Task } from '@/work/types'
import type { Unit } from '@/planning/types'
import { TooltipProvider } from '@/ui/primitives/tooltip'
import type { ReactNode } from 'react'

declare global { var IS_REACT_ACT_ENVIRONMENT: boolean }
globalThis.IS_REACT_ACT_ENVIRONMENT = true

const turn = (question: string, answer: string | null, more: Partial<Turn> = {}): Turn => ({ question, context: [], answer, at: '2026-11-21T10:00:00Z', ...more })
const discussion: Thread = { id: 'q1', ended: false, live: false, discuss: true, turns: [turn('Why IP?', 'THE_ANSWER', { answerBy: 'agent-R', answeredAt: '2026-11-21T10:01:00Z', unread: true })] }
const plain = (k: number): Thread => ({ id: `q${k + 2}`, ended: false, live: false, turns: [turn(`Plain ${k}`, 'ok')] })

let host: HTMLDivElement
let root: Root
const show = (node: ReactNode) => act(() => root.render(<TooltipProvider>{node}</TooltipProvider>))
beforeEach(() => { vi.useFakeTimers(); host = document.createElement('div'); document.body.append(host); root = createRoot(host) })
afterEach(() => { act(() => root.unmount()); host.remove(); vi.useRealTimers() })

describe('the discussion in the Questions block', () => {
  it('is shown, open, behind three newer conversations, and read only once its answer is on screen', () => {
    const onRead = vi.fn()
    show((<Questions id="q" threads={[discussion, plain(0), plain(1), plain(2)]} agent="agent-R" onFile={() => {}} onRead={onRead} />))
    act(() => { vi.advanceTimersByTime(2000) })
    expect(host.textContent).toContain('THE_ANSWER')
    expect(onRead).toHaveBeenCalledWith('2026-11-21T10:01:00Z')
  })

  it('folded by the user, is not read', () => {
    const onRead = vi.fn()
    show((<Questions id="q" threads={[discussion]} agent="agent-R" onFile={() => {}} onRead={onRead} />))
    act(() => { (host.querySelector('.thread-row') as HTMLButtonElement).click() })
    act(() => { vi.advanceTimersByTime(2000) })
    expect(host.textContent).not.toContain('THE_ANSWER')
    expect(onRead).not.toHaveBeenCalled()
  })
})

describe('Draft conclusion', () => {
  const unit = { id: 'survey', title: 'Survey', tasks: [] } as unknown as Unit
  const task = (threads: Thread[]) => ({ id: '1', taskId: '1', agent: 'agent-R', discuss: 'open', output: { gate: '2' }, threads }) as unknown as Task
  const talk = { onDiscuss: vi.fn(() => Promise.resolve()), onOpen: () => {} } as unknown as Talk
  const offered = (threads: Thread[]) => {
    show(<Discussion unit={unit} n={task(threads)} talk={talk} />)
    return [...host.querySelectorAll('button')].some(b => /conclusion|结论|結論/i.test(b.textContent ?? ''))
  }

  it('is offered once the result has been discussed, not before', () => {
    expect(offered([])).toBe(false)
    expect(offered([plain(0)])).toBe(false)
    expect(offered([discussion])).toBe(true)
  })
})
