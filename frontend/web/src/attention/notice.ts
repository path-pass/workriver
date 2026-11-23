// What blocks on the user, said in one line (the Manager conversation's bar, a notification),
// and which of it is news: a row of Blocking as it comes, told once.
import type { T } from '@/i18n'
import type { InboxRow } from './select'

/** A row of Blocking in one line: the agent waiting, the stalled worker, the gate's title. */
export function blockingLine(t: T, waited: (since: number) => string, r: InboxRow): string {
  if (r.agent) return t('inbox.blockedAgent', { agent: r.agent.name })
  const n = r.node!
  if (r.kind === 'stalled') {
    const task = `${r.unit!.id}#${n.taskId ?? n.id}`
    const agent = n.agent ?? ''
    return n.stalled?.gone ? t('inbox.stalledGone', { task, agent }) : t('inbox.stalled', { task, agent, ago: waited(r.since) })
  }
  return n.title
}

/** A notification's title and body for a row of Blocking: the agent and its topic, or what
 *  waits and where (its intent and unit). */
export function noticeOf(t: T, waited: (since: number) => string, r: InboxRow): { title: string; body?: string } {
  if (r.agent) return { title: t('agents.notice', { agent: r.agent.name }), body: r.agent.topic }
  const body = [r.intent?.title, r.unit?.title].filter(Boolean).join(' / ')
  if (r.kind === 'gate') return { title: t('inbox.gateNotice', { gate: r.node!.title }), body }
  return { title: blockingLine(t, waited, r), body }
}

/**
 * Says, for each list of Blocking's rows in turn, which rows came that were not in the list
 * before (by key: a row that only changes is not news). The first list is what was there when
 * the page loaded: not news. A row that left and comes back is news again.
 */
export function blockingNews() {
  let seen: Set<string> | null = null
  return (rows: InboxRow[]): InboxRow[] => {
    const before = seen
    seen = new Set(rows.map(r => r.key))
    return before ? rows.filter(r => !before.has(r.key)) : []
  }
}
