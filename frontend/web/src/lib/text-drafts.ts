import { useLayoutEffect, useState } from 'react'

type Key = string | null
type Editor = (key: Key, text: string) => void

/** Text outlives its editor. Only editors of the changed draft hear a write. */
export function createTextDrafts() {
  const drafts = new Map<Key, string>()
  const editors = new Set<Editor>()
  const read = (key: Key) => drafts.get(key) ?? ''
  return {
    read,
    write(key: Key, text: string) {
      if (read(key) === text) return
      if (text) drafts.set(key, text)
      else drafts.delete(key)
      editors.forEach(edit => edit(key, text))
    },
    subscribe(key: Key, update: (text: string) => void) {
      const edit: Editor = (changed, text) => { if (changed === key) update(text) }
      editors.add(edit)
      update(read(key))
      return () => { editors.delete(edit) }
    },
  }
}

export type TextDrafts = ReturnType<typeof createTextDrafts>

/** Each mounted editor keeps local state, including the Manager's two responsive copies. */
export function useTextDraft(drafts: TextDrafts, key: Key) {
  const [draft, setDraft] = useState(() => ({ key, text: drafts.read(key) }))
  useLayoutEffect(() => drafts.subscribe(key, text => {
    setDraft(previous => previous.key === key && previous.text === text ? previous : { key, text })
  }), [drafts, key])
  return [draft.key === key ? draft.text : drafts.read(key), (text: string) => drafts.write(key, text)] as const
}
