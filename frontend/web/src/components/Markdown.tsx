// A Markdown attachment, rendered. Its text is an agent's, so untrusted: react-markdown builds
// React elements (never an HTML string), raw HTML in it is dropped, unsafe link protocols are
// removed (its default urlTransform), links open in a new tab without access to this one, and
// images are not fetched: each is a link to where it points. Loaded only when one is opened
// (react-markdown with remark-gfm, about 50 kB gzip), so the main bundle does not carry it.
import Markdown from 'react-markdown'
import remarkGfm from 'remark-gfm'
import { useT } from '@/i18n'

export default function MarkdownView({ text }: { text: string }) {
  const { t } = useT()
  return (
    <div className="md">
      <Markdown
        remarkPlugins={[remarkGfm]}
        skipHtml
        components={{
          a: ({ href, children }) => <a href={href} target="_blank" rel="noopener noreferrer">{children}</a>,
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
