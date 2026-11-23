// Markdown an agent wrote, rendered: an attachment in the viewer, or (compact) a task's input,
// a delivery note, an answer. Its text is untrusted: react-markdown builds React elements
// (never an HTML string), raw HTML in it is dropped, unsafe link protocols are removed (its
// default urlTransform), external links open in a new tab without access to this one, and images are
// not fetched: each is a link to where it points. A ```mermaid block is drawn as a diagram,
// shown only as an image (Diagram.tsx). Loaded only when one is shown (react-markdown with
// remark-gfm, about 50 kB gzip), so the main bundle does not carry it.
import type { ComponentProps } from 'react'
import Markdown, { type ExtraProps } from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { pathOf, placeOf } from '@/app/link'
import { contentLang, useT } from '@/i18n'
import { Diagram } from './diagram/Diagram'

/** File paths in the text: the same pattern answers used before they were Markdown. */
const PATH = /[\w-]+(?:\/[\w.-]+)+\.\w+/g
const WHOLE_PATH = new RegExp(`^${PATH.source}$`)
/** A link to a file of the unit, made by `filePaths`; never fetched. */
const FILE = '#file:'
const WORKRIVER_PATH = /^\/(?:[iu]\d+\/?|u\d+\/\d+\/?(?:\?finding=[^&#]+)?)$/

function workriverPath(href: string | undefined): string | undefined {
  if (!href || href.startsWith('#')) return
  let url: URL
  try { url = new URL(href, location.href) } catch { return }
  // The CLI prints a loopback URL even when the page is served at another origin.
  const loopback = url.protocol === 'http:' && url.hostname === '127.0.0.1'
  const path = url.pathname + url.search + url.hash
  if (!(url.origin === location.origin || loopback) || !WORKRIVER_PATH.test(path)) return
  const place = placeOf(url.pathname, url.search)
  if (place) return pathOf(place.id, place.task ? { kind: 'task', id: place.task, finding: place.finding } : null)
}

type MdNode = { type: string; value?: string; url?: string; children?: MdNode[] }

/** A remark plugin: paths in text and in inline code become links that pick the file. */
function filePaths() {
  const link = (path: string, child: MdNode): MdNode => ({ type: 'link', url: FILE + path, children: [child] })
  const walk = (node: MdNode) => {
    if (!node.children || node.type === 'link' || node.type === 'linkReference') return
    node.children = node.children.flatMap(c => {
      if (c.type === 'inlineCode' && WHOLE_PATH.test(c.value ?? '')) return [link(c.value!, c)]
      if (c.type !== 'text') { walk(c); return [c] }
      const text = c.value ?? '', out: MdNode[] = []
      let last = 0
      for (const m of text.matchAll(PATH)) {
        out.push({ type: 'text', value: text.slice(last, m.index) }, link(m[0], { type: 'text', value: m[0] }))
        last = m.index + m[0].length
      }
      return out.length ? [...out, { type: 'text', value: text.slice(last) }] : [c]
    })
  }
  return (tree: unknown) => walk(tree as MdNode)
}

/** The text of a ```mermaid block, without the line break that ends it; of any other, nothing. */
function mermaidCode(node: ExtraProps['node']): string | undefined {
  const code = node?.children[0]
  if (node?.children.length !== 1 || code?.type !== 'element' || code.tagName !== 'code') return undefined
  const lang = code.properties.className
  if (!Array.isArray(lang) || !lang.includes('language-mermaid')) return undefined
  return code.children.map(c => (c.type === 'text' ? c.value : '')).join('').replace(/\n$/, '')
}

/** A code block, or a mermaid block's diagram. Defined once, so a diagram drawn stays when the text re-renders. */
function Pre({ node, children }: ComponentProps<'pre'> & ExtraProps) {
  const code = mermaidCode(node)
  return code === undefined ? <pre>{children}</pre> : <Diagram code={code} />
}

export default function MarkdownView({ text, compact = false, onFile }: { text: string; compact?: boolean; onFile?: (path: string) => void }) {
  const { t } = useT()
  return (
    <div className={compact ? 'md md-sm' : 'md'} lang={contentLang(text)}>
      <Markdown
        remarkPlugins={onFile ? [remarkGfm, filePaths] : [remarkGfm]}
        skipHtml
        components={{
          pre: Pre,
          a: ({ href, children }) => {
            if (onFile && href?.startsWith(FILE)) {
              return <button type="button" className="md-file" onClick={() => onFile(href.slice(FILE.length))}>{children}</button>
            }
            const path = workriverPath(href)
            if (!path) return <a href={href} target="_blank" rel="noopener noreferrer">{children}</a>
            return <a href={path} onClick={event => {
              if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return
              event.preventDefault()
              if (path === location.pathname + location.search + location.hash) return
              history.pushState(null, '', path)
              window.dispatchEvent(new PopStateEvent('popstate'))
            }}>{children}</a>
          },
          img: ({ src, alt }) => {
            const where = typeof src === 'string' ? src : ''
            const label = t('attach.mdImage', { name: alt || where })
            return where ? <a href={where} target="_blank" rel="noopener noreferrer">{label}</a> : <span>{label}</span>
          },
        }}
      >
        {text}
      </Markdown>
    </div>
  )
}
