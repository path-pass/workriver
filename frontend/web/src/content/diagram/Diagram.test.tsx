// A ```mermaid block in Markdown an agent wrote is drawn as a diagram. What mermaid drew is
// untrusted like the text: the page gets it only as an image (no script, no link, nothing
// fetched), and a block mermaid cannot draw shows its code as before, with a note.
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Picture } from './draw'

const draw = vi.hoisted(() => ({
  calls: [] as { code: string; dark: boolean }[],
  answer: (_code: string): Promise<Picture> => new Promise(() => {}),
}))
vi.mock('./draw', () => ({
  drawDiagram: (code: string, dark: boolean) => { draw.calls.push({ code, dark }); return draw.answer(code) },
}))
const { default: MarkdownView } = await import('../Markdown')

declare global { var IS_REACT_ACT_ENVIRONMENT: boolean }
globalThis.IS_REACT_ACT_ENVIRONMENT = true

const FLOW = '```mermaid\ngraph TD\n  A-->B\n```'
// What a hostile block could make mermaid put out: none of it may run or be fetched.
const HOSTILE = '<svg xmlns="http://www.w3.org/2000/svg" width="100%" viewBox="0 0 320 180"><script>alert(1)</script>'
  + '<a href="javascript:alert(2)"><text onclick="alert(3)">A</text></a><image href="https://example.invalid/x.png"/>'
  + '<foreignObject><img src="https://example.invalid/y.png" onerror="alert(4)"/></foreignObject></svg>'

let host: HTMLDivElement
let root: ReturnType<typeof createRoot>
beforeEach(() => {
  host = document.createElement('div'); document.body.append(host); root = createRoot(host)
  draw.calls = []
})
afterEach(() => { act(() => root.unmount()); host.remove(); delete document.documentElement.dataset.theme })

async function show(text: string) {
  await act(async () => { root.render(<MarkdownView text={text} />) })
  await act(async () => { await new Promise(r => setTimeout(r)) })
}

describe('a mermaid block in Markdown', () => {
  it('puts a wide image at its drawn width inside the diagram wrapper', async () => {
    draw.answer = () => Promise.resolve({ svg: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1800 180"/>', width: 1800, height: 180 })
    await show(FLOW)
    expect(host.querySelector('.md > .md-diagram > img')?.getAttribute('width')).toBe('1800')
  })

  it('is handed to the diagram renderer; no other code block is', async () => {
    draw.answer = () => new Promise(() => {})
    await show(`${FLOW}\n\n\`\`\`js\nconst a = 1\n\`\`\`\n\n\`\`\`\nplain\n\`\`\`\n\nand \`mermaid\` inline`)
    expect(draw.calls).toEqual([{ code: 'graph TD\n  A-->B', dark: false }])
    // Until it is drawn, its code shows as any code block does.
    expect(host.querySelectorAll('pre code')[0].textContent).toBe('graph TD\n  A-->B\n')
  })

  it('is shown as an image only: nothing of what was drawn runs or is fetched', async () => {
    const fetched = vi.spyOn(globalThis, 'fetch')
    draw.answer = () => Promise.resolve({ svg: HOSTILE, width: 320, height: 180 })
    await show(FLOW)
    const img = host.querySelectorAll('img')
    expect(img).toHaveLength(1)
    expect(img[0].getAttribute('src')).toMatch(/^data:image\/svg\+xml;charset=utf-8,%3Csvg/)
    expect([img[0].getAttribute('width'), img[0].getAttribute('height')]).toEqual(['320', '180'])
    expect(host.querySelector('svg, script, a, iframe, image, foreignObject, [onclick], [onerror]')).toBeNull()
    expect(host.innerHTML.replace(img[0].getAttribute('src')!, '')).not.toContain('example.invalid')
    expect(fetched).not.toHaveBeenCalled()
    fetched.mockRestore()
  })

  it('is drawn once: a re-render of the same text keeps the diagram', async () => {
    draw.answer = () => Promise.resolve({ svg: HOSTILE, width: 320, height: 180 })
    await show(FLOW)
    await show(FLOW)
    expect(draw.calls).toHaveLength(1)
    expect(host.querySelectorAll('img')).toHaveLength(1)
  })

  it('too heavy to draw is never handed to mermaid: its code shows, under a note', async () => {
    const heavy = 'graph TD\n' + Array.from({ length: 150 }, (_, i) => `N0-->N${i + 1}`).join('\n')
    await show('```mermaid\n' + heavy + '\n```')
    expect(draw.calls).toEqual([])
    expect(host.querySelector('img')).toBeNull()
    expect(host.querySelector('pre code')?.textContent).toBe(heavy + '\n')
    expect(host.textContent).toContain('This diagram could not be drawn; its text is below.')
  })

  it('of a kind not drawn here (a gantt chart) is never handed to mermaid: its code shows, under a note', async () => {
    const gantt = 'gantt\n  excludes weekends\n  t0 :2024-01-01, 12000d'
    await show('```mermaid\n' + gantt + '\n```')
    expect(draw.calls).toEqual([])
    expect(host.querySelector('pre code')?.textContent).toBe(gantt + '\n')
    expect(host.textContent).toContain('This diagram could not be drawn; its text is below.')
  })

  it('that mermaid cannot draw shows its code with a note, and nothing throws', async () => {
    draw.answer = () => Promise.reject(new Error('Parse error on line 1'))
    await show('```mermaid\ngraph TD\n  A--\n```')
    expect(host.querySelector('img')).toBeNull()
    expect(host.querySelector('pre code')?.textContent).toBe('graph TD\n  A--\n')
    expect(host.textContent).toContain('This diagram could not be drawn')
    expect(host.textContent).not.toContain('Parse error')
  })

  it("follows the page's theme", async () => {
    draw.answer = () => Promise.resolve({ svg: HOSTILE, width: 320, height: 180 })
    document.documentElement.dataset.theme = 'dark'
    await show(FLOW)
    draw.answer = () => new Promise(() => {})
    document.documentElement.dataset.theme = 'light'
    await act(async () => { await new Promise(r => setTimeout(r)) })
    expect(draw.calls.map(c => c.dark)).toEqual([true, false])
    // The diagram drawn before stays until the new one is ready.
    expect(host.querySelectorAll('img')).toHaveLength(1)
  })
})
