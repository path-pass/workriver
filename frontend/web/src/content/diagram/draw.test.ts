// Mermaid draws in a frame of its own, one diagram at a time, under rules a block cannot
// change: strict security, no HTML labels, and a page that may fetch nothing.
import { describe, expect, it } from 'vitest'
import { FRAME, inTurn, size } from './draw'
import { BOUNDS } from './bounds'
import { mermaidConfig } from './config'

const later = () => new Promise(r => setTimeout(r))

describe('drawing one diagram at a time', () => {
  it('starts the next only when the one before has finished, failed or not', async () => {
    const log: string[] = []
    let finish!: () => void
    const first = inTurn(() => { log.push('1'); return new Promise<void>((_, fail) => { finish = () => fail(new Error('x')) }) })
    const second = inTurn(async () => { log.push('2') })
    await later()
    expect(log).toEqual(['1'])
    finish()
    await expect(first).rejects.toThrow('x')
    await second
    expect(log).toEqual(['1', '2'])
  })

  it('skips one no longer wanted when its turn comes', async () => {
    const log: string[] = []
    let finish!: () => void
    const first = inTurn(() => new Promise<void>(done => { finish = done }))
    const gone = new AbortController()
    const skipped = inTurn(async () => { log.push('ran') }, gone.signal)
    await later()
    gone.abort()
    finish()
    await first
    await expect(skipped).rejects.toThrow()
    expect(log).toEqual([])
  })
})

describe('the size of what mermaid drew', () => {
  it("is the root's viewBox; without one it is not a diagram", () => {
    expect(size('<svg id="d0" width="100%" xmlns="http://www.w3.org/2000/svg" style="max-width: 812.5px;" viewBox="-8 -8 812.5 300.25"><g/></svg>')).toEqual({ width: 813, height: 301 })
    expect(size('<svg width="100%"><g viewBox="0 0 10 10"/></svg>')).toBeUndefined()
    expect(size('not svg')).toBeUndefined()
  })
})

describe("mermaid's settings", () => {
  it('strict, no HTML labels, dagre layout, the theme of the page; a block cannot change them', () => {
    for (const dark of [false, true]) {
      const c = mermaidConfig(dark)
      expect(c.securityLevel).toBe('strict')
      expect(c.htmlLabels).toBe(false)
      expect(c.startOnLoad).toBe(false)
      expect(c.theme).toBe(dark ? 'dark' : 'neutral')
      // ELK, mermaid 12's default, is left out of the build (vite.config.ts).
      expect(c.layout).toBe('dagre')
      // Its own limits no looser than ours (bounds.ts), checked before it draws.
      expect([c.maxTextSize, c.maxEdges]).toEqual([BOUNDS.chars, BOUNDS.weight])
      // And every other key the frame names (all of mermaid's): nothing in a block changes them.
      expect(mermaidConfig(dark, ['flowchart', 'dompurifyConfig', 'look']).secure).toEqual(expect.arrayContaining(['flowchart', 'dompurifyConfig', 'look', 'theme']))
      for (const key of ['securityLevel', 'htmlLabels', 'layout', 'dompurifyConfig', 'theme', 'themeCSS', 'themeVariables', 'fontFamily', 'altFontFamily', 'darkMode'])
        expect(c.secure).toContain(key)
    }
  })

  it('the frame it draws in may run only its own script and fetch nothing', () => {
    const page = new DOMParser().parseFromString(FRAME('http://127.0.0.1:1/assets/frame.js'), 'text/html')
    const csp = page.querySelector('head meta[http-equiv="Content-Security-Policy"]')?.getAttribute('content')
    expect([...page.scripts].map(s => [s.type, s.src])).toEqual([['module', 'http://127.0.0.1:1/assets/frame.js']])
    expect(csp?.split(';').map(s => s.trim()).filter(Boolean)).toEqual(["default-src 'none'", "script-src 'self'", "style-src 'unsafe-inline'"])
  })
})
