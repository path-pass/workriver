// What the user can do to the data (submit a gate, decide a finding, finish a chore), handed
// down from the shell to wherever it is done.
import { createContext, useContext } from 'react'
import type { Decision, GateResult, Finding, Task } from './types'
import type { Delivery } from '@/api/actions'
import type { Unit } from '@/planning/types'

export interface Act {
  submitGate: (unit: Unit, gate: Task, r: GateResult) => void
  decide: (unit: Unit, task: Task, f: Finding, d: Decision, note: string) => void
  /** Takes an input back while its message has not gone out. */
  undo: (key: string) => void
  choreDone: (id: string) => void
  /** Merges planned intents into `into`; rejects with the service's words. */
  merge: (into: string, from: string[], title?: string) => Promise<void>
  /** By the same key as the input: `unit:task` for a gate, `unit:task:finding` for a decision. */
  delivery: Record<string, Delivery>
  message: Record<string, string>
}

export const ActContext = createContext<Act | null>(null)
export const useAct = () => useContext(ActContext)!
export const gateKey = (unit: Unit, task: Task) => `${unit.id}:${task.id}`
export const findingKey = (unit: Unit, task: Task, f: Finding) => `${unit.id}:${task.id}:${f.id}`
