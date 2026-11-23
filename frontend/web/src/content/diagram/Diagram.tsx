// A ```mermaid block of Markdown an agent wrote, drawn as a diagram (draw.ts). What
// mermaid drew is shown only as an image: an SVG image runs no script, fetches nothing and
// has no links. Until it is drawn, and when mermaid cannot draw it or it is not drawn here (a
// kind not drawn, or too heavy: bounds.ts; then mermaid is never asked), the block
// shows its code as any code block does, with a diagnostic or note on failure.
// The diagram follows the page's theme.
// Its own file apart from draw.ts: its tests fake the drawing by replacing that module.
import { useEffect, useMemo, useState, useSyncExternalStore } from 'react'
import { useT } from '@/i18n'
import { BOUNDS, refusal } from './bounds'
import { drawDiagram, MermaidError, type Picture } from './draw'

const darkSystem = () => matchMedia('(prefers-color-scheme: dark)')
/** As index.css decides it: data-theme on <html>, else the system. */
const isDark = () => {
  const picked = document.documentElement.dataset.theme
  return picked ? picked === 'dark' : darkSystem().matches
}
function watchTheme(changed: () => void) {
  const marks = new MutationObserver(changed)
  marks.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] })
  const system = darkSystem()
  system.addEventListener('change', changed)
  return () => { marks.disconnect(); system.removeEventListener('change', changed) }
}

export function Diagram({ code }: { code: string }) {
  const { t, num } = useT()
  const dark = useSyncExternalStore(watchTheme, isDark, () => false)
  // For this code: what was drawn last (kept while it is drawn again in the other theme), or failed.
  const [got, setGot] = useState<{ code: string; picture?: Picture; error?: string }>()
  const refused = useMemo(() => refusal(code), [code])
  useEffect(() => {
    if (refused) return
    const shown = new AbortController()
    drawDiagram(code, dark, shown.signal).then(
      picture => { if (!shown.signal.aborted) setGot({ code, picture }) },
      (error: unknown) => {
        if (!shown.signal.aborted) setGot({ code, error: error instanceof MermaidError ? error.message : '' })
      },
    )
    return () => shown.abort()
  }, [code, dark, refused])
  const now = got?.code === code ? got : undefined
  if (now?.picture) {
    const { svg, width, height } = now.picture
    return (
      <div className="md-diagram">
        <img src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`} width={width} height={height} alt={t('attach.mdDiagram')} />
      </div>
    )
  }
  let note: string | undefined
  if (refused?.reason === 'kind') {
    note = t('attach.mdDiagramUnsupported')
  } else if (refused) {
    const keys = { chars: 'attach.mdDiagramChars', word: 'attach.mdDiagramWord', weight: 'attach.mdDiagramWeight' } as const
    note = t(keys[refused.reason], { actual: num(refused.actual), limit: num(BOUNDS[refused.reason]) })
  } else if (now?.error !== undefined) {
    note = now.error || t('attach.mdDiagramFailed')
  }
  return (
    <>
      {note && <p className={now?.error ? 'md-note font-mono' : 'md-note'}>{note}</p>}
      <pre><code className="language-mermaid">{`${code}\n`}</code></pre>
    </>
  )
}
