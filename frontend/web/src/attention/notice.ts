// Which waiting agents are news.
import type { WaitingAgentRow } from './select'

/**
 * Finds newly waiting agents by row key. The first list is what was there when the page
 * loaded. An agent that leaves the list and comes back is news again.
 */
export function waitingAgentNews() {
  let seen: Set<string> | null = null
  return (rows: WaitingAgentRow[]): WaitingAgentRow[] => {
    const before = seen
    seen = new Set(rows.map(r => r.key))
    return before ? rows.filter(r => !before.has(r.key)) : []
  }
}
