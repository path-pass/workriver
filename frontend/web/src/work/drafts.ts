import { createContext, useContext, useState } from 'react'
import type { Attachment } from '@/attachments/types'
import type { GateResult, Severity } from './types'

export type GateDraft = {
  result: GateResult['result']
  picked?: string
  note?: string
  files: Attachment[]
  drafts: { severity: Severity; problem: string; impact: string; where: string; files: Attachment[] }[]
}

type Drafts = {
  gates: GateDraft
  decisions: string
  drops: string
  discussions: { text: string; files: Attachment[] }
}
type TaskDrafts = { [K in keyof Drafts]: Map<string, Drafts[K]> }

/** Drafts live for the page, even when the task detail or a finding's row closes. */
export function useTaskDrafts() {
  const [drafts] = useState<TaskDrafts>(() => ({
    gates: new Map(),
    decisions: new Map(),
    drops: new Map(),
    discussions: new Map(),
  }))
  return drafts
}

export const TaskDraftsContext = createContext<TaskDrafts | null>(null)

export function useTaskDraft<K extends keyof Drafts>(kind: K, key: string) {
  const store = useContext(TaskDraftsContext)![kind]
  // Each key has one editor. Only that editor needs to render when it writes.
  const [, refresh] = useState(0)
  const save = (next?: Drafts[K]) => {
    if (next === undefined) store.delete(key)
    else store.set(key, next)
    refresh(n => n + 1)
  }
  return [store.get(key), save] as const
}
