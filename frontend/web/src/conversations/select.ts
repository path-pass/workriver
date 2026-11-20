// A task's conversations, with what was asked in this sitting, and how far one has got.
import type { Task } from '@/work/types'
import type { Thread, Turn } from './types'
import type { Talk } from './types'

/** Recorded conversations, with questions asked in this sitting that the data does not have yet going with the last. */
export function withAsked(list: Thread[], extra: Turn[]): Thread[] {
  if (!extra.length) return list
  const last = list.at(-1)
  return last && !last.ended ? [...list.slice(0, -1), { ...last, turns: [...last.turns, ...extra] }] : [...list, { id: '', ended: false, live: false, turns: extra }]
}

/** A task's conversations: from the service, or the sample's single one. */
export function threadsOf(n: Task, talk: Talk): Thread[] {
  return withAsked(n.threads ?? (n.thread?.length ? [{ id: '', ended: false, live: false, turns: n.thread }] : []), talk.asked[n.id] ?? [])
}

export const turnsOf = (n: Task, talk: Talk) => threadsOf(n, talk).flatMap(t => t.turns)

/** How far a conversation has got, from its last exchange. */
export function stateOf(th: Thread): 'ended' | 'failed' | 'waiting' | 'answered' {
  const last = th.turns.at(-1)
  return th.ended ? 'ended' : last?.failed ? 'failed' : last && last.answer === null ? 'waiting' : 'answered'
}
