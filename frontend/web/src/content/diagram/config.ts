// Mermaid's settings in the frame it draws in (frame.ts). 'strict': labels are
// sanitized and a click directive calls nothing. Mermaid's own 'sandbox' level is not used: its
// iframe allows top navigation by a click and popups, and it does not stop what mermaid
// fetches while drawing (an image shape loads its URL); the frame and the image the page shows
// do both. No HTML labels: every label is SVG text. Flowcharts laid out with dagre, not
// mermaid 12's default ELK (1.5 MB), which the build leaves out (vite.config.ts). A block's
// own settings never reach mermaid: the frame takes its front matter and %%{…}%% directives
// away first (bounds.ts `withoutSettings`). `secure`, which would refuse such settings
// by their top-level names only, stays as a second line: these, DOMPurify's, and the
// top-level keys of mermaid's defaults the frame adds.
// Its own file: tested without the frame, which acts on import.
import type { MermaidConfig } from 'mermaid'
import { BOUNDS } from './bounds'

export function mermaidConfig(dark: boolean, keys: string[] = []): MermaidConfig {
  return {
    startOnLoad: false,
    securityLevel: 'strict',
    htmlLabels: false,
    layout: 'dagre',
    // Mermaid's own limits, no looser than ours (bounds.ts), which are checked first.
    maxTextSize: BOUNDS.chars,
    maxEdges: BOUNDS.weight,
    theme: dark ? 'dark' : 'neutral',
    darkMode: dark,
    // System fonts only (nothing to fetch), the same that the image is shown with.
    fontFamily: '-apple-system, BlinkMacSystemFont, "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", "Segoe UI", sans-serif',
    secure: ['secure', 'securityLevel', 'startOnLoad', 'maxTextSize', 'suppressErrorRendering', 'maxEdges',
      'htmlLabels', 'layout', 'theme', 'themeCSS', 'themeVariables', 'fontFamily', 'altFontFamily', 'darkMode', 'dompurifyConfig', ...keys],
  }
}
