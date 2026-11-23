// What an agent wrote is untrusted: the renderer drops raw HTML, keeps unsafe link
// protocols out, opens links without access to this page, and never fetches an image.
import { flushSync } from 'react-dom'
import { createRoot } from 'react-dom/client'
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'
import MarkdownView from './Markdown'
import { NavigateContext } from './navigation'

const html = (text: string, onFile?: (path: string) => void) => renderToStaticMarkup(<MarkdownView text={text} onFile={onFile} />)

describe('Markdown an agent wrote', () => {
  it('drops raw HTML, inline and as a block', () => {
    const out = html('Hi <script>alert(1)</script> <b onclick="x()">bold</b>\n\n<div><img src=x onerror="alert(2)"></div>\n\n<iframe src="https://example.invalid"></iframe>')
    for (const bad of ['<script', 'onclick', 'onerror', '<img', '<iframe', '<div><img', '<b']) expect(out).not.toContain(bad)
    expect(out).toContain('Hi ')
  })

  it('keeps unsafe link protocols out of links', () => {
    for (const url of ['javascript:alert(1)', 'JaVaScRiPt:alert(1)', 'data:text/html,<script>alert(1)</script>', 'vbscript:msgbox(1)']) {
      const out = html(`[click](${url})`)
      expect(out).not.toMatch(/href="(javascript|data|vbscript):/i)
      expect(out).toContain('click')
    }
  })

  it('opens links in a new tab with no access to this one', () => {
    const out = html('[docs](https://example.invalid/u1/3) and <https://example.invalid/auto>')
    expect(out).toContain('<a href="https://example.invalid/u1/3" target="_blank" rel="noopener noreferrer">docs</a>')
    expect(out).toContain('<a href="https://example.invalid/auto" target="_blank" rel="noopener noreferrer">')
  })

  it.each([
    ['http://127.0.0.1:8000/u1/3。在 11/20 更新', '/u1/3', 'http://127.0.0.1:8000/u1/3', '。在 11/20 更新'],
    ['（https://example.invalid/docs）', 'https://example.invalid/docs', 'https://example.invalid/docs', '）'],
    ['https://example.invalid/docsはこちら', 'https://example.invalid/docs', 'https://example.invalid/docs', 'はこちら'],
    ['https://example.invalid日本', 'https://example.invalid', 'https://example.invalid', '日本'],
    ['https://example.invalid。次へ', 'https://example.invalid', 'https://example.invalid', '。次へ'],
    ['www.example.invalid/docs【続き】', 'http://www.example.invalid/docs', 'www.example.invalid/docs', '【続き】'],
    ['https&colon;//example.invalid/docs日本', 'https://example.invalid/docs', 'https://example.invalid/docs', '日本'],
    ['https://a日本.jp/docsはこちら', 'https://a%E6%97%A5%E6%9C%AC.jp/docs', 'https://a日本.jp/docs', 'はこちら'],
    ['https://例え.テスト/docsはこちら', 'https://%E4%BE%8B%E3%81%88.%E3%83%86%E3%82%B9%E3%83%88/docs', 'https://例え.テスト/docs', 'はこちら'],
    ['https://example.invalid/docs.', 'https://example.invalid/docs', 'https://example.invalid/docs', '.'],
    ['“https://example.invalid/docs”', 'https://example.invalid/docs', 'https://example.invalid/docs', '”'],
    ['‘https://example.invalid/docs’', 'https://example.invalid/docs', 'https://example.invalid/docs', '’'],
    ['https://example.invalid/docs“说明”', 'https://example.invalid/docs', 'https://example.invalid/docs', '“说明”'],
    ['https://example.invalid/docs‘说明’', 'https://example.invalid/docs', 'https://example.invalid/docs', '‘说明’'],
    ['https://example.invalid/docs——说明', 'https://example.invalid/docs', 'https://example.invalid/docs', '——说明'],
    ['https://example.invalid/docs……', 'https://example.invalid/docs', 'https://example.invalid/docs', '……'],
    ['https://example.invalid/docs·说明', 'https://example.invalid/docs', 'https://example.invalid/docs', '·说明'],
  ])('ends a bare link at its boundary in %s', (markdown, href, label, trailing) => {
    const host = document.createElement('div')
    host.innerHTML = html(markdown)
    const link = host.querySelector('a')!
    expect(link.getAttribute('href')).toBe(href)
    expect(link.textContent).toBe(label)
    expect(link.nextSibling?.textContent).toBe(trailing)
  })

  it.each([
    ['https://one.invalid/docs、https://two.invalid/docs', [
      ['https://one.invalid/docs', 'https://one.invalid/docs'],
      ['https://two.invalid/docs', 'https://two.invalid/docs'],
    ]],
    ['https://one.invalid/docs和www.two.invalid/docs，http://three.invalid/docs。', [
      ['https://one.invalid/docs', 'https://one.invalid/docs'],
      ['http://www.two.invalid/docs', 'www.two.invalid/docs'],
      ['http://three.invalid/docs', 'http://three.invalid/docs'],
    ]],
  ])('links each bare URL in %s', (markdown, links) => {
    const host = document.createElement('div')
    host.innerHTML = html(markdown)
    expect(Array.from(host.querySelectorAll('a'), link => [link.getAttribute('href'), link.textContent])).toEqual(links)
    expect(host.textContent).toBe(markdown)
  })

  it.each([
    ['https://日本/', 'https://%E6%97%A5%E6%9C%AC/'],
    ['https://日本.jp/', 'https://%E6%97%A5%E6%9C%AC.jp/'],
    ['https://www.日本.jp/', 'https://www.%E6%97%A5%E6%9C%AC.jp/'],
    ['https://l·l.cat/docs', 'https://l%C2%B7l.cat/docs'],
  ])('keeps the complete internationalized host in %s', (markdown, href) => {
    expect(html(markdown)).toContain(`<a href="${href}" target="_blank" rel="noopener noreferrer">${markdown}</a>`)
  })

  it.each([
    '[docs](https://example.invalid/日本)',
    '<https://example.invalid/日本>',
  ])('keeps the complete explicit link in %s', markdown => {
    expect(html(markdown)).toContain('href="https://example.invalid/%E6%97%A5%E6%9C%AC" target="_blank"')
  })

  it.each([
    'http://127.0.0.1:8000/u1/3?view=docs',
    'http://127.0.0.1:8000/u1/3#install',
    'http://127.0.0.1:8000/u1/3?view=docs#install',
  ])('keeps route-shaped references such as %s in a new tab', href => {
    expect(html(`[docs](${href})`)).toContain(`<a href="${href}" target="_blank" rel="noopener noreferrer">docs</a>`)
  })

  it.each([
    ['http://127.0.0.1:8000/u1/3?finding=P2-1', '/u1/3?finding=P2-1'],
    ['http://127.0.0.1:8000/u1/3。続き', '/u1/3'],
    [`[open](${location.origin}/u1/)`, '/u1'],
    ['[open](/i3)', '/i3'],
    ['[open](/u1/3/?finding=two+words)', '/u1/3?finding=two%20words'],
  ])('passes %s to the page on a plain click', (text, path) => {
    const before = location.href
    const entries = history.length
    const host = document.createElement('div')
    const root = createRoot(host)
    const destinations: string[] = []
    const navigate = (destination: string) => destinations.push(destination)
    try {
      flushSync(() => root.render(<NavigateContext.Provider value={navigate}><MarkdownView text={text} /></NavigateContext.Provider>))
      const link = host.querySelector('a')!
      let intercepted = false
      host.addEventListener('click', event => {
        intercepted = event.defaultPrevented
        event.preventDefault()
      })
      for (const keys of [{ metaKey: true }, { ctrlKey: true }, { shiftKey: true }, { altKey: true }, { button: 1 }]) {
        link.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, ...keys }))
        expect(intercepted).toBe(false)
      }
      expect(destinations).toEqual([])

      link.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }))
      expect(destinations).toEqual([path])
      expect(intercepted).toBe(true)
      expect(link.href).toBe(`${location.origin}${path}`)
      expect(link.target).toBe('')

      expect(history.length).toBe(entries)
      expect(location.href).toBe(before)
    } finally {
      root.unmount()
    }
  })

  it('never fetches an image: it becomes a link to where it points', () => {
    const out = html('![a chart](https://example.invalid/chart.png)\n\n![](https://example.invalid/b.png)\n\n![none]()')
    expect(out).not.toContain('<img')
    expect(out).toContain('<a href="https://example.invalid/chart.png" target="_blank" rel="noopener noreferrer">Image: a chart</a>')
    expect(out).toContain('Image: https://example.invalid/b.png</a>')
    expect(out).toContain('<span>Image: none</span>')
  })

  it("turns the unit's file paths into buttons only when a file can be picked", () => {
    expect(html('See src/app/main.ts and `docs/a.md`.', () => {})).toMatch(/<button type="button" class="md-file">src\/app\/main\.ts<\/button>.*<button type="button" class="md-file"><code>docs\/a\.md<\/code><\/button>/)
    expect(html('See src/app/main.ts.')).not.toContain('<button')
  })

  it('names the language of Chinese or Japanese content for its glyphs', () => {
    expect(html('用中文写的')).toMatch(/^<div class="md" lang="zh-CN">/)
    expect(html('ひらがなと漢字')).toMatch(/^<div class="md" lang="ja">/)
    expect(html('plain')).toMatch(/^<div class="md">/)
  })
})
