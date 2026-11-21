// A file another open unit of the project also changes is marked in the code tree, with the
// units and their intents on hover; a folder does not sum it; the file's own view names those
// units as links that open them.
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import i18n from 'i18next'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { TooltipProvider } from '@/ui/primitives/tooltip'
import { selectionOf } from '@/work/select'
import type { Intent, Unit } from '@/planning/types'
import { CodeLayer } from './CodeLayer'
import { alsoChanged, changeOf } from './files'
import type { FileChange, ModuleNode } from './types'

declare global { var IS_REACT_ACT_ENVIRONMENT: boolean }
globalThis.IS_REACT_ACT_ENVIRONMENT = true

const change = (path: string, before: string, after: string): FileChange =>
  ({ path, status: 'M', before, after, ...changeOf(before, after), beforeMode: '100644', afterMode: '100644', beforeId: before, afterId: after })
const unit = {
  id: 'u1', commits: [{ id: 'c1', subject: 'Store', date: '2026-11-21T00:00:00Z', author: 'a', live: false, files: [change('app/store.ts', 'a\n', 'b\n'), change('app/view.ts', 'a\n', 'b\n')] }],
  tasks: [{ id: '1', taskId: '1', kind: 'implement', state: 'done', title: 'Store', plans: [], inputs: [], output: { commits: ['c1'] } }],
  overlaps: [{ path: 'app/store.ts', units: ['u2', 'u3'] }],
} as unknown as Unit
const intents = [
  { id: 'i1', title: 'Search', units: [unit] },
  { id: 'i2', title: 'Login', units: [{ id: 'u2' }] },
  { id: 'i3', title: 'Export', units: [{ id: 'u3' }] },
] as unknown as Intent[]
const root: ModuleNode = { name: '', path: '', loc: 2, files: 2, children: [{ name: 'app', path: 'app', loc: 2, files: 2, children: [
  { name: 'store.ts', path: 'app/store.ts', loc: 1, files: 1 }, { name: 'view.ts', path: 'app/view.ts', loc: 1, files: 1 },
] }] }

let host: HTMLDivElement
beforeEach(async () => { await i18n.changeLanguage('en'); host = document.createElement('div'); document.body.append(host) })
afterEach(() => host.remove())

function shown(file: string | null, opened: string[] = []) {
  const view = createRoot(host)
  act(() => view.render(
    <TooltipProvider>
      <CodeLayer
        root={root} unit={unit} sel={selectionOf(unit, null)} onlyChanged onOnlyChanged={() => {}} onPick={() => {}}
        file={file} onFile={() => {}} lines={null} onLines={() => {}} onAsk={() => {}} commit={null}
        also={alsoChanged(intents, unit)} onUnit={id => opened.push(id)}
      />
    </TooltipProvider>,
  ))
  return view
}

describe('a file another open unit also changes', () => {
  it('is marked in the tree, with the units and their intents; a folder and another file are not', () => {
    const r = shown(null)
    const rows = [...host.querySelectorAll('.trow')]
    const row = (name: string) => rows.find(b => b.querySelector('.nm')?.textContent?.startsWith(name))!
    expect(row('store.ts').querySelector('[data-also] [role="img"]')?.getAttribute('aria-label')).toBe('Also changed by u2 (i2 Login), u3 (i3 Export)')
    expect(row('view.ts').querySelector('[data-also]')).toBeNull()
    expect(row('app').querySelector('[data-also]')).toBeNull()
    act(() => r.unmount())
  })

  it('in its own view, names the units as links that open them', () => {
    const opened: string[] = []
    const r = shown('app/store.ts', opened)
    // The tree's mark is a span; the file's header line is a block of its own.
    const line = host.querySelector('div[data-also]')!
    for (const part of ['Also changed by', 'i2 Login', 'i3 Export']) expect(line.textContent).toContain(part)
    const links = [...line.querySelectorAll('a')]
    expect(links.map(a => a.getAttribute('href'))).toEqual(['/u2', '/u3'])
    act(() => links[1].dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, button: 0 })))
    expect(opened).toEqual(['u3'])
    act(() => r.unmount())
  })

  it('a unit no other open unit overlaps has no mark at all', () => {
    expect(alsoChanged(intents, { ...unit, overlaps: undefined }).size).toBe(0)
  })
})
