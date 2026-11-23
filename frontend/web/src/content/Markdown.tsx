// Markdown an agent wrote, rendered: an attachment in the viewer, or (compact) a task's input,
// a delivery note, an answer. Its text is untrusted: react-markdown builds React elements
// (never an HTML string), raw HTML in it is dropped, unsafe link protocols are removed (its
// default urlTransform), external links open in a new tab without access to this one, and images are
// not fetched: each is a link to where it points. A ```mermaid block is drawn as a diagram,
// shown only as an image (Diagram.tsx). Loaded only when one is shown (react-markdown with
// remark-gfm, about 50 kB gzip), so the main bundle does not carry it.
import { useContext, type ComponentProps } from 'react'
import Markdown, { type ExtraProps } from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { contentLang, useT } from '@/i18n'
import { Diagram } from './diagram/Diagram'
import { NavigateContext } from './navigation'

/** File paths in the text: the same pattern answers used before they were Markdown. */
const PATH = /[\w-]+(?:\/[\w.-]+)+\.\w+/g
const WHOLE_PATH = new RegExp(`^${PATH.source}$`)
/** A link to a file of the unit, made by `filePaths`; never fetched. */
const FILE = '#file:'
const WORKRIVER_PATH = /^\/(?:[iu]\d+\/?|u\d+\/\d+\/?(?:\?finding=[^&#]+)?)$/
const CJK_BOUNDARY = /[·—‘’“”…\p{Script=Han}\p{Script_Extensions=Hiragana}\p{Script_Extensions=Katakana}\u3000-\u303f\uff01-\uff0f\uff1a-\uff20\uff3b-\uff40\uff5b-\uff65]/u
const NEXT_BARE_URL = /(?<!\w)(?:https?:\/\/|www\.)(?=[\p{L}\p{N}])/iu
// Host labels can be internationalized. An ASCII final label can run into CJK prose.
// A middle dot between two l's can belong to a Catalan host label.
const HOST_PREFIX = /^(?:https?:\/\/)?(?:(?:[\p{L}\p{M}\p{N}-]|(?<=l)·(?=l))+\.)*(?:[a-z0-9-]+|(?=\P{ASCII})[\p{L}\p{M}\p{N}-]+)/iu

function workriverPath(href: string | undefined): string | undefined {
  if (!href || href.startsWith('#')) return
  let url: URL
  try { url = new URL(href, location.href) } catch { return }
  // The CLI prints a loopback URL even when the page is served at another origin.
  const loopback = url.protocol === 'http:' && url.hostname === '127.0.0.1'
  const path = url.pathname + url.search + url.hash
  if (!(url.origin === location.origin || loopback) || !WORKRIVER_PATH.test(path)) return
  const route = url.pathname.replace(/\/$/, '')
  const finding = url.searchParams.get('finding')
  return finding ? `${route}?finding=${encodeURIComponent(finding)}` : route
}

type MdNode = { type: string; value?: string; url?: string; children?: MdNode[]; position?: { start: { offset?: number } } }

/** End GFM bare URLs where Chinese or Japanese prose begins. */
function cjkAutolinks() {
  const walk = (node: MdNode) => {
    if (!node.children || node.type === 'linkReference') return
    node.children = node.children.flatMap(child => {
      if (child.type !== 'link') { walk(child); return [child] }
      const label = child.children?.[0]
      if (child.children?.length !== 1 || label?.type !== 'text' || !label.value || !child.url || !/^https?:\/\//i.test(child.url)) return [child]
      // GFM literals share the label's start, or have no positions after GFM's text transform.
      if (child.position?.start.offset !== label.position?.start.offset) return [child]
      const nodes: MdNode[] = []
      let link = child, linkLabel = label
      let value = label.value, url = child.url
      while (true) {
        nodes.push(link)
        const host = value.match(HOST_PREFIX)?.[0] ?? ''
        const end = value.slice(host.length).search(CJK_BOUNDARY)
        if (end === -1) return nodes
        const trailing = value.slice(host.length + end)
        linkLabel.value = value.slice(0, -trailing.length)
        link.url = url.slice(0, -trailing.length)

        // GFM consumed the whole run, including any later URLs in the prose.
        const next = trailing.search(NEXT_BARE_URL)
        if (next === -1) return [...nodes, { type: 'text', value: trailing }]
        nodes.push({ type: 'text', value: trailing.slice(0, next) })
        value = trailing.slice(next)
        url = /^www\./i.test(value) ? 'http://' + value : value
        linkLabel = { type: 'text', value }
        link = { type: 'link', url, children: [linkLabel] }
      }
    })
  }
  return (tree: unknown) => walk(tree as MdNode)
}

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
  const navigate = useContext(NavigateContext)
  return (
    <div className={compact ? 'md md-sm' : 'md'} lang={contentLang(text)}>
      <Markdown
        remarkPlugins={onFile ? [remarkGfm, cjkAutolinks, filePaths] : [remarkGfm, cjkAutolinks]}
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
              if (!navigate || event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return
              event.preventDefault()
              navigate(path)
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
