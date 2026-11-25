// The gate, mounted against a service made of `fetch`: the screen is shown as text (nothing in
// it becomes Markdown, a link or a colour), in the order a terminal draws it, in the region,
// the whole screen and the option buttons; a cut region opens with the whole screen; a press
// waits for the one before it; a failed read leaves the keys disabled; the generic keys fold
// under "Other keys" when the dialog was read and are open when it was not.
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { TooltipProvider } from '@/ui/primitives/tooltip'
import { Gate, sends } from './Gate'
import { AgentContext } from './context'
import { liveSession, refOf } from './select'
import { InboxList } from '@/attention/Inbox'
import type { AgentRef, Dialog, ScreenView, Session } from './types'
import type { Workspace } from '@/workspace/types'

declare global { var IS_REACT_ACT_ENVIRONMENT: boolean }
globalThis.IS_REACT_ACT_ENVIRONMENT = true

const s: Session = { agent: 'ja-impl', session: 's-1', ended: false, state: 'blocked', blockedSince: new Date(Date.now() - 5 * 60000).toISOString() }
const ws = { repos: [], builtAt: '', runtime: 'Runtime-x', intents: [], sessions: [s] } as Workspace

/** A screen whose last lines are a numbered dialog; `above` the lines over its region. */
function screen(lines: string[], dialog: Partial<Dialog> | null, digest = 'd1'): ScreenView {
  return {
    text: lines.join('\n'), cols: 120, rows: lines.length, digest,
    dialog: dialog && { options: [], numbered: true, hints: [], above: null, question: null, top: 0, bottom: lines.length - 1, cut: false, ...dialog },
  }
}
const item = (number: number | null, words: string, row: number, marked = false) => ({ number, words, more: [], marked, row })

const BIDI = 'echo ⟨U+202E⟩txt.exe  שלום עולם'
const proceed = screen(
  ['$ make', '**bold** [link](http://evil.example) <a href="x">a</a> \u001b[31mred', '─────', ' Bash command', `   ${BIDI}`, ' Do you want to proceed?', ' ❯ 1. Yes', '   2. No, ⟨U+202E⟩gnp.exe', ' Esc to cancel'],
  { options: [item(1, 'Yes', 6, true), item(2, 'No, ⟨U+202E⟩gnp.exe', 7)], hints: [{ key: 'escape', words: 'Esc to cancel' }], question: 'Do you want to proceed?', top: 3, bottom: 8 },
)

// The service, as `fetch` sees it: queued answers per route, and what the page sent.
let answers: Record<string, (() => Promise<Response>)[]>
let sent: { url: string; body?: unknown }[]
const json = (body: unknown, status = 200) => () => Promise.resolve(new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } }))
beforeEach(() => {
  answers = { screen: [], keys: [] }
  sent = []
  vi.stubGlobal('fetch', vi.fn((url: string, init?: RequestInit) => {
    sent.push({ url, body: init?.body ? JSON.parse(String(init.body)) : undefined })
    const next = answers[url.includes('/keys') ? 'keys' : 'screen'].shift()
    return next ? next() : Promise.reject(new Error('no answer'))
  }))
})
afterEach(() => vi.unstubAllGlobals())

let root: Root, host: HTMLDivElement
beforeEach(() => { host = document.createElement('div'); document.body.append(host); root = createRoot(host) })
afterEach(() => { act(() => root.unmount()); host.remove() })

const settle = () => act(() => new Promise<void>(r => setTimeout(r, 0)))
async function open() {
  act(() => root.render(<TooltipProvider><Gate ws={ws} s={s} onClose={() => {}} onOpen={() => {}} onView={() => {}} /></TooltipProvider>))
  await settle()
  return document.querySelector<HTMLElement>('[role=dialog]')!
}
const texts = (d: HTMLElement) => [...d.querySelectorAll<HTMLElement>('[data-terminal-text]')]
const button = (d: HTMLElement, name: string) => [...d.querySelectorAll('button')].find(b => (b.getAttribute('aria-label') ?? b.textContent ?? '').includes(name))!
const genericKeys = (d: HTMLElement) => [...d.querySelectorAll<HTMLButtonElement>('button[aria-label^="Press "]')]

describe('the gate', () => {
  it('shows the screen as plain text: Markdown, links, tags and escape sequences stay inert', async () => {
    answers.screen.push(json(proceed))
    const d = await open()
    // The whole screen is folded (the region was not cut); unfolding it shows every line.
    act(() => button(d, 'Whole screen').click())
    const whole = texts(d).find(e => e.textContent!.includes('$ make'))!
    expect(whole.textContent).toContain('**bold** [link](http://evil.example) <a href="x">a</a> \u001b[31mred')
    expect(d.querySelector('a, strong, em, code')).toBeNull()
    expect(whole.tagName).toBe('PRE')
  })

  it('keeps the written order, with the marks visible, in the region, the whole screen and the buttons', async () => {
    answers.screen.push(json(proceed))
    const d = await open()
    act(() => button(d, 'Whole screen').click())
    const region = texts(d).find(e => e.textContent!.startsWith(' Bash command'))!
    const whole = texts(d).find(e => e.textContent!.startsWith('$ make'))!
    const option = [...d.querySelectorAll('ul[aria-label=Answers] [data-terminal-text]')].find(e => e.textContent!.includes('gnp'))!
    for (const e of [region, whole, option] as HTMLElement[]) {
      expect(e.style.direction).toBe('ltr')
      expect(e.style.unicodeBidi).toBe('bidi-override')
      expect(e.getAttribute('dir')).toBe('ltr')
    }
    expect(region.textContent).toContain(BIDI)
    expect(whole.textContent).toContain(BIDI)
    expect(option.textContent).toBe('No, ⟨U+202E⟩gnp.exe')
  })

  it('shows each option with what it sends: its digit, or arrows from the marked one then Enter', async () => {
    answers.screen.push(json(proceed))
    const d = await open()
    const caps = [...d.querySelectorAll('ul[aria-label=Answers] kbd')].map(k => k.textContent)
    expect(caps).toEqual(['1', '2'])
    const list = { ...proceed.dialog!, numbered: false, options: [item(null, 'a', 1), item(null, 'b', 2, true), item(null, 'c', 3), item(null, 'd', 4)] }
    expect([0, 1, 2, 3].map(i => sends(list, i).join(' '))).toEqual(['up enter', 'enter', 'down enter', 'down down enter'])
  })

  it('moves and toggles a checklist before confirming on its final row', async () => {
    const checklist = (rows: string[], digest: string, submit = false) => screen(
      ['  2 new MCP servers found in this project', ...rows, ' Space to select · Esc to reject all'],
      {
        checklist: true, numbered: false,
        options: [item(null, 'alpha', 1), item(null, 'beta', 2), item(null, 'Enable selected', 3)],
        hints: [submit ? { key: 'enter', words: 'Enable selected' } : { key: 'space', words: 'Space to select' }, { key: 'escape', words: 'Esc to reject all' }],
      }, digest,
    )
    const initial = checklist(['  ❯ [✔] alpha', '    [✔] beta', '       Enable selected'], 'initial')
    const moved = checklist(['    [✔] alpha', '  ❯ [✔] beta', '       Enable selected'], 'moved')
    const toggled = checklist(['    [✔] alpha', '  ❯ [ ] beta', '       Enable selected'], 'toggled')
    const ready = checklist(['    [✔] alpha', '    [ ] beta', '  ❯    Enable selected'], 'ready', true)
    answers.screen.push(json(initial))
    answers.keys.push(
      ...[moved, toggled, ready].map((next, i) => json({ outcome: 'open', keys: [i === 1 ? 'space' : 'down'], screen: next })),
      json({ outcome: 'answered', keys: ['enter'] }),
    )
    const d = await open()
    expect(d.querySelector('ul[aria-label=Answers] button')).toBeNull()
    expect(button(d, 'Enable selected')).toBeUndefined()
    for (const label of ['Press Down', 'Space to select', 'Press Down', 'Enable selected']) {
      act(() => button(d, label).click())
      await settle()
      if (label === 'Space to select') expect(texts(d).some(e => e.textContent!.includes('❯ [ ] beta'))).toBe(true)
    }
    expect(sent.slice(1).map(r => r.body)).toEqual([
      { agent: 's-1', shown: 'initial', press: { key: 'down' } },
      { agent: 's-1', shown: 'moved', press: { key: 'space' } },
      { agent: 's-1', shown: 'toggled', press: { key: 'down' } },
      { agent: 's-1', shown: 'ready', press: { key: 'enter' } },
    ])
  })

  it('opens with the whole screen when the region was cut, saying why', async () => {
    const long = screen(['line 1', 'line 2', ' ❯ 1. Yes', '   2. No'], { options: [item(1, 'Yes', 2, true), item(2, 'No', 3)], top: 2, bottom: 3, cut: true })
    answers.screen.push(json(long))
    const d = await open()
    expect(texts(d).some(e => e.textContent === long.text)).toBe(true)
    expect(d.textContent).toContain('The question is longer than the lines shown above')
  })

  it('folds the generic keys under "Other keys" when the dialog was read', async () => {
    answers.screen.push(json(proceed))
    const d = await open()
    expect(genericKeys(d)).toEqual([])
    act(() => button(d, 'Other keys').click())
    expect(genericKeys(d).map(b => b.textContent)).toEqual(['↑', '↓', '↵', 'Esc', 'Tab', '1', '2', '3', '4', '5', '6', '7', '8', '9'])
  })

  it('shows them open, and the terminal view standing out, when no dialog was read', async () => {
    answers.screen.push(json(screen(['some output', '> '], null)))
    const d = await open()
    expect(genericKeys(d)).toHaveLength(14)
    expect(genericKeys(d).every(b => !b.disabled)).toBe(true)
    expect(button(d, 'Open terminal view').dataset.variant).toBe('default')
  })

  it('disables the keys when the screen could not be read, says why and offers to try again', async () => {
    answers.screen.push(json({ code: 'runtime', reason: 'the screen read timed out' }, 503))
    const d = await open()
    expect(d.textContent).toContain('Its screen could not be read: The runtime cannot read or answer it now.')
    expect(genericKeys(d)).toHaveLength(14)
    expect(genericKeys(d).every(b => b.disabled)).toBe(true)
    answers.screen.push(json(proceed))
    act(() => button(d, 'Try again').click())
    await settle()
    expect(d.querySelectorAll('ul[aria-label=Answers] button')).toHaveLength(2)
  })

  it('sends a press with the digest of the screen shown, and waits for it before another', async () => {
    answers.screen.push(json(proceed))
    const d = await open()
    let answer!: (r: Response) => void
    answers.keys.push(() => new Promise(r => { answer = r }))
    act(() => button(d, 'No, ').click())
    const options = [...d.querySelectorAll<HTMLButtonElement>('ul[aria-label=Answers] button')]
    expect(options.every(b => b.disabled)).toBe(true)
    expect(button(d, 'Esc to cancel').hasAttribute('disabled')).toBe(true)
    expect(sent.at(-1)).toEqual({ url: '/api/agent/keys', body: { agent: 's-1', shown: 'd1', press: { option: 2 } } })
    // Still open after it: the new screen and its buttons, one line saying so; the next press goes with its digest.
    const after = screen(['x', ' ❯ 1. Yes', '   2. No'], { options: [item(1, 'Yes', 1, true), item(2, 'No', 2)], top: 1, bottom: 2 }, 'd2')
    await act(async () => { answer(new Response(JSON.stringify({ outcome: 'open', keys: ['2'], screen: after }), { status: 200 })) })
    await settle()
    expect(d.textContent).toContain('Sent; the question is still open.')
    expect([...d.querySelectorAll<HTMLButtonElement>('ul[aria-label=Answers] button')].every(b => !b.disabled)).toBe(true)
    answers.keys.push(json({ code: 'changed', reason: 'the screen changed', screen: { ...after, digest: 'd3' } }, 409))
    act(() => button(d, 'Yes').click())
    await settle()
    expect(sent.at(-1)!.body).toMatchObject({ shown: 'd2', press: { option: 1 } })
    expect(d.textContent).toContain('The screen changed; look again.')
  })
})

describe('an agent by its session and machine', () => {
  // The same resumed session lives on this machine and on "box": each row reaches its own.
  const here: Session = { ...s, group: { id: 'w1', label: 'here-ws' } }
  const box: Session = { ...s, machine: 'box', group: { id: 'w2', label: 'box-ws', machine: 'box' } }
  const two = { ...ws, sessions: [here, box] } as Workspace

  it('opens the gate and the terminal view of the one picked, and keeps it after a refresh', async () => {
    const asked: AgentRef[] = []
    const actions = { answer: (r: AgentRef) => asked.push(r), view: (r: AgentRef) => asked.push(r) }
    act(() => root.render(
      <TooltipProvider><AgentContext.Provider value={actions}><InboxList ws={two} onOpen={() => {}} onMachines={() => {}} onTalk={() => {}} /></AgentContext.Provider></TooltipProvider>,
    ))
    const row = [...host.querySelectorAll<HTMLElement>('.irow-pair')].find(e => e.textContent!.includes('box-ws'))!
    act(() => row.querySelector<HTMLButtonElement>('button[data-kind=agent]')!.click())
    act(() => row.querySelector<HTMLButtonElement>('button[aria-label^="Open the terminal"]')!.click())
    expect(asked.map(r => r.machine)).toEqual(['box', 'box'])
    const picked = liveSession(two.sessions, asked[0])!
    expect(picked.machine).toBe('box')
    answers.screen.push(json(proceed))
    act(() => root.render(<TooltipProvider><Gate ws={two} s={picked} onClose={() => {}} onOpen={() => {}} onView={() => {}} /></TooltipProvider>))
    await settle()
    expect(sent[0].url).toBe('/api/agent/screen?agent=s-1&machine=box')
    // A refresh brings new objects, this machine's first: still the one on "box".
    const again = [{ ...here }, { ...box }]
    expect(liveSession(again, refOf(picked))?.machine).toBe('box')
    expect(liveSession(again, refOf(here))?.machine).toBeUndefined()
  })
})
