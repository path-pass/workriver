// What an agent wrote is untrusted: the renderer drops raw HTML, keeps unsafe link
// protocols out, opens links without access to this page, and never fetches an image.
import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'
import MarkdownView from './Markdown'

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
    const out = html('[docs](https://example.invalid/docs) and <https://example.invalid/auto>')
    expect(out).toContain('<a href="https://example.invalid/docs" target="_blank" rel="noopener noreferrer">docs</a>')
    expect(out).toContain('<a href="https://example.invalid/auto" target="_blank" rel="noopener noreferrer">')
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
