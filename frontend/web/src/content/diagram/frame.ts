// Runs in the frame the diagrams are drawn in (draw.ts): takes a block's text from the
// page, draws it with mermaid and sends back the SVG text or a diagnostic. The page
// shows the SVG only as an image (Diagram.tsx).
// Its own file: a separate build entry that acts on import, and the only one to import mermaid.
import mermaid from 'mermaid'
import { refusal, withoutSettings } from './bounds'
import { mermaidConfig } from './config'

/** Mermaid's SVG is HTML text (an `xlink:href` with its prefix undeclared, say); an image
 * needs well-formed XML. A template's content is inert: nothing in it loads or runs. */
function asXml(svg: string): string {
  const t = document.createElement('template')
  t.innerHTML = svg
  const root = t.content.firstElementChild
  if (root?.localName !== 'svg') throw new Error('nothing drawn')
  return new XMLSerializer().serializeToString(root)
}

/** Mermaid's names for the kinds drawn (bounds.ts). */
const KINDS = new Set(['flowchart-v2', 'sequence', 'classDiagram', 'stateDiagram', 'er'])

let n = 0
addEventListener('message', async (e: MessageEvent<{ id: number; code: string; dark: boolean }>) => {
  if (e.source !== parent || e.origin !== origin) return
  const { id, code, dark } = e.data
  try {
    // The page refuses these already; nothing of another kind or too heavy reaches mermaid
    // in any case, and the kind is the one mermaid itself finds.
    if (refusal(code)) throw new Error('not drawn here')
    // Mermaid is never given a block's own settings (front matter, directives): ours stand.
    const text = withoutSettings(code)
    mermaid.initialize(mermaidConfig(dark, Object.keys(mermaid.mermaidAPI.defaultConfig)))
    if (!KINDS.has(mermaid.detectType(text))) throw new Error('not drawn here')
    const result = await mermaid.render(`d${n++}`, text).catch((error: unknown) => {
      parent.postMessage({ id, error: error instanceof Error ? error.message : undefined }, origin)
    })
    if (!result) return
    const { svg } = result
    // What mermaid draws in place of a diagram it refuses is not one.
    if (/aria-roledescription="error"|Maximum text size in diagram exceeded/.test(svg)) throw new Error('refused')
    parent.postMessage({ id, svg: asXml(svg) }, origin)
  } catch {
    parent.postMessage({ id, failed: true }, origin)
  } finally {
    // What a failed render can leave behind; one is drawn at a time.
    for (const el of [...document.body.children]) if (el.tagName !== 'SCRIPT') el.remove()
  }
})
parent.postMessage({ ready: true }, origin)
