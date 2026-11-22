// Text off an agent's screen, drawn in the order a terminal draws it. A browser reorders text
// by the Unicode bidi algorithm and a terminal does not, so a command could read one way here
// and run another: every character goes left to right in the order it was written
// (`direction: ltr; unicode-bidi: bidi-override`). The marks that reorder or hide text come
// already made visible by the service (⟨U+202E⟩). Plain text only: never Markdown, no links.
// The one place this is done: the gate's region, its whole screen, its option buttons and the
// recorded answer all use it.
import type { CSSProperties } from 'react'
import { cn } from '@/lib/utils'

const ORDER: CSSProperties = { direction: 'ltr', unicodeBidi: 'bidi-override' }

/** One run of screen text; `block` keeps its lines (a region, a whole screen). */
export function TerminalText({ text, block = false, className }: { text: string; block?: boolean; className?: string }) {
  if (block) return <pre dir="ltr" data-terminal-text="" style={ORDER} className={cn('font-mono whitespace-pre', className)}>{text}</pre>
  return <span dir="ltr" data-terminal-text="" style={ORDER} className={className}>{text}</span>
}
