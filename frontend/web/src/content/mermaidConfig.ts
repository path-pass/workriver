// Mermaid's settings in the frame it draws in (diagramFrame.ts). 'strict': labels are
// sanitized and a click directive calls nothing. Mermaid's own 'sandbox' level is not used: its
// iframe allows top navigation by a click and popups, and it does not stop what mermaid
// fetches while drawing (an image shape loads its URL); the frame and the image the page shows
// do both. No HTML labels: every label is SVG text. Flowcharts laid out with dagre, not
// mermaid 12's default ELK (1.5 MB), which the build leaves out (vite.config.ts). `secure`
// keeps a block's %%{init}%% directive or front matter from changing any of this, the theme
// or the fonts.
import type { MermaidConfig } from 'mermaid'
import { BOUNDS } from './diagramBounds'

export function mermaidConfig(dark: boolean): MermaidConfig {
  return {
    startOnLoad: false,
    securityLevel: 'strict',
    htmlLabels: false,
    layout: 'dagre',
    // Mermaid's own limits, no looser than ours (diagramBounds.ts), which are checked first.
    maxTextSize: BOUNDS.chars,
    maxEdges: BOUNDS.weight,
    theme: dark ? 'dark' : 'neutral',
    darkMode: dark,
    // System fonts only (nothing to fetch), the same that the image is shown with.
    fontFamily: '-apple-system, BlinkMacSystemFont, "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", "Segoe UI", sans-serif',
    secure: ['secure', 'securityLevel', 'startOnLoad', 'maxTextSize', 'suppressErrorRendering', 'maxEdges',
      'htmlLabels', 'layout', 'theme', 'themeCSS', 'themeVariables', 'fontFamily', 'altFontFamily', 'darkMode'],
  }
}
