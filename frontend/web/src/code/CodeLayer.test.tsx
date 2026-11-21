// A file's change in the list and the single-file view: a binary file says so as git does
// ("Binary files differ"), a submodule pointer names its two commits; neither has line counts.
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { en } from '@/i18n/en'
import '@/i18n'
import { FileDiff } from './CodeLayer'
import type { FileChange } from './types'

const GONE = '000000', NULL = '0'.repeat(40), A = 'a'.repeat(40), B = 'b'.repeat(40)
const file = (more: Partial<FileChange>): FileChange => ({
  path: 'x', status: 'M', before: null, after: null, add: 0, del: 0, lines: [], beforeMode: '100644', beforeId: A, afterMode: '100644', afterId: B, ...more,
})

let root: Root
let el: HTMLDivElement
beforeEach(() => { el = document.createElement('div'); document.body.append(el); root = createRoot(el) })
afterEach(() => { act(() => root.unmount()); el.remove() })
const show = (f: FileChange, whole = false) => { act(() => root.render(<FileDiff file={f} whole={whole} />)); return el.textContent ?? '' }

describe('a change with no lines', () => {
  for (const whole of [false, true]) {
    it(`a binary file: "Binary files differ", no counts (${whole ? 'single file' : 'list'})`, () => {
      const text = show(file({ path: 'logo.png', beforeBinary: true, afterBinary: true }), whole)
      expect(text).toContain('logo.png')
      expect(text).toContain(en.change.binary)
      expect(text).not.toMatch(/\+0|−0/)
    })

    it(`a submodule pointer: its two commits, no counts (${whole ? 'single file' : 'list'})`, () => {
      const text = show(file({ path: 'vendor/lib', beforeMode: '160000', afterMode: '160000' }), whole)
      expect(text).toContain(A.slice(0, 9))
      expect(text).toContain(B.slice(0, 9))
      expect(text).not.toMatch(/\+0|−0/)
    })
  }

  it('a submodule added or removed names the commit on its one side', () => {
    expect(show(file({ status: 'A', beforeMode: GONE, beforeId: NULL, afterMode: '160000' }))).toContain(B.slice(0, 9))
    expect(show(file({ status: 'D', beforeMode: '160000', afterMode: GONE, afterId: NULL }))).toContain(A.slice(0, 9))
  })

  it('a file or a binary file made a submodule: the pointer, never "Binary files differ" nor lines', () => {
    for (const was of [{ before: 'one\n', del: 0 }, { beforeBinary: true }]) {
      const text = show(file({ path: 'vendor/lib', ...was, afterMode: '160000' }))
      expect(text).toContain(B.slice(0, 9))
      expect(text).not.toContain(en.change.binary)
      expect(text).not.toMatch(/\+0|−0|−1/)
    }
  })

  it('a mode change alone keeps its +0 −0', () => {
    expect(show(file({ path: 'run.sh', afterMode: '100755' }))).toMatch(/\+0 −0/)
  })
})
