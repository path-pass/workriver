import { createContext, useContext, useState, type ReactElement } from 'react'
import type { Attachment } from '@/attachments/types'
import { createTextDrafts, type TextDrafts } from '@/lib/text-drafts'
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
  discussions: Attachment[]
}
type DraftValues = { [K in keyof Drafts]: Map<string, Drafts[K]> }
type TaskDrafts = DraftValues & { discussionText: TextDrafts }

/** Drafts live for the page, even when the task detail or a finding's row closes. */
export function useTaskDrafts() {
  const [drafts] = useState<TaskDrafts>(() => ({
    gates: new Map(),
    decisions: new Map(),
    drops: new Map(),
    discussions: new Map(),
    discussionText: createTextDrafts(),
  }))
  return drafts
}

export const TaskDraftsContext = createContext<TaskDrafts | null>(null)

export function useTaskDraft<K extends keyof Drafts>(kind: K, key: string) {
  const drafts: DraftValues = useContext(TaskDraftsContext)!
  const store = drafts[kind]
  // Each key has one editor. Only that editor needs to render when it writes.
  const [, refresh] = useState(0)
  const save = (next?: Drafts[K], render = true) => {
    if (next === undefined) store.delete(key)
    else store.set(key, next)
    if (render) refresh(n => n + 1)
  }
  return [store.get(key), save, () => store.get(key)] as const
}

/** Read and write a saved draft without rendering the enclosing form on each keystroke. */
export function DraftText({ read, save, children }: {
  read: () => string
  save: (next: string) => void
  children: (text: string, onText: (next: string) => void) => ReactElement
}) {
  const [, refresh] = useState(0)
  return children(read(), next => { save(next); refresh(n => n + 1) })
}
