// How an attachment previews: as a picture, as Markdown, as plain text, or not at all.
import type { Attachment } from './types'
type Kind = 'image' | 'markdown' | 'text'
const MD = /\.(md|markdown|mdown|mkd)$/i
const TEXT = /\.(txt|text|log|out|json|jsonl|ndjson|ya?ml|toml|ini|cfg|conf|csv|tsv|xml|diff|patch|sql|sh|env)$/i

/** How an attachment previews, or null: it only downloads. SVG never previews as a document;
 *  without a type (the sample's pictures) it is a picture. */
export function kindOf(a: Attachment): Kind | null {
  const type = a.type ?? ''
  if (type === 'image/svg+xml') return null
  if (type.startsWith('image/')) return 'image'
  if (type === 'text/markdown' || MD.test(a.name)) return 'markdown'
  if (type.startsWith('text/') || /^application\/(.+\+)?(json|xml|yaml|x-ndjson)$/.test(type) || TEXT.test(a.name)) return 'text'
  return type ? null : 'image'
}
