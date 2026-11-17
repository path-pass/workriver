// Markdown an agent wrote, rendered: an attachment in the viewer, or (compact) a task's input,
// a delivery note, an answer. Its text is untrusted: react-markdown builds React elements
// (never an HTML string), raw HTML in it is dropped, unsafe link protocols are removed (its
// default urlTransform), links open in a new tab without access to this one, and images are
// not fetched: each is a link to where it points. Loaded only when one is shown (react-markdown
// with remark-gfm, about 50 kB gzip), so the main bundle does not carry it.
import Markdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { useT } from '@/i18n'

/** File paths in the text: the same pattern answers used before they were Markdown. */
const PATH = /[\w-]+(?:\/[\w.-]+)+\.\w+/g
const WHOLE_PATH = new RegExp(`^${PATH.source}$`)
/** A link to a file of the unit, made by `filePaths`; never fetched. */
const FILE = '#file:'

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

export default function MarkdownView({ text, compact = false, onFile }: { text: string; compact?: boolean; onFile?: (path: string) => void }) {
  const { t } = useT()
  return (
    <div className={compact ? 'md md-sm' : 'md'}>
      <Markdown
        remarkPlugins={onFile ? [remarkGfm, filePaths] : [remarkGfm]}
        skipHtml
        components={{
          a: ({ href, children }) =>
            onFile && href?.startsWith(FILE)
              ? <button type="button" className="md-file" onClick={() => onFile(href.slice(FILE.length))}>{children}</button>
              : <a href={href} target="_blank" rel="noopener noreferrer">{children}</a>,
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
