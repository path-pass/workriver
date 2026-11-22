// What opens an agent's gate (its question answered from the page) and its terminal view:
// the shell supplies them; a row, a line or a button anywhere asks for them.
import { createContext } from 'react'
import type { AgentRef } from './types'

export interface AgentActions {
  /** Opens the gate of an agent waiting for the user in its terminal. */
  answer: (a: AgentRef) => void
  /** Opens its terminal view (one per page: another agent's replaces it). */
  view: (a: AgentRef) => void
}

export const AgentContext = createContext<AgentActions>({ answer: () => {}, view: () => {} })
