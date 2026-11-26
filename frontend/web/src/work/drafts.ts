import { createContext, useEffect, useState, useSyncExternalStore } from 'react'
import { detach } from '@/api/actions'
import type { Attachment } from '@/attachments/types'

function draftStore<T>() {
  const entries = new Map<string, T>()
  const listeners = new Set<() => void>()
  return {
    entries,
    subscribe(listener: () => void) {
      listeners.add(listener)
      return () => { listeners.delete(listener) }
    },
    save(key: string, value?: T) {
      if (value === undefined) entries.delete(key)
      else entries.set(key, value)
      listeners.forEach(listener => listener())
    },
  }
}

/** Drafts live for the page, even when the task detail or a finding's row closes. */
export function useTaskDrafts() {
  const [drafts] = useState(() => ({
    decisions: draftStore<string>(),
    drops: draftStore<string>(),
    discussions: draftStore<{ text: string; files: Attachment[] }>(),
  }))
  useEffect(() => () => {
    for (const d of drafts.discussions.entries.values()) d.files.forEach(detach)
  }, [drafts])
  return drafts
}

export const TaskDraftsContext = createContext<ReturnType<typeof useTaskDrafts> | null>(null)

export function useTaskDraft<T>(store: ReturnType<typeof draftStore<T>>, key: string) {
  const draft = useSyncExternalStore(store.subscribe, () => store.entries.get(key))
  return [draft, (next?: T) => store.save(key, next)] as const
}
