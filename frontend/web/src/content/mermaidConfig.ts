// Mermaid's settings in the frame it draws in (diagramFrame.ts). 'strict': labels are
// sanitized and a click directive calls nothing. Mermaid's own 'sandbox' level is not used: its
// iframe allows top navigation by a click and popups, and it does not stop what mermaid
// fetches while drawing (an image shape loads its URL); the frame and the image the page shows
// do both. No HTML labels: every label is SVG text. `secure` keeps a block's %%{init}%%
// directive from changing any of this, the theme or the fonts.
import type { MermaidConfig } from 'mermaid'

export function mermaidConfig(dark: boolean): MermaidConfig {
  return {
    startOnLoad: false,
    securityLevel: 'strict',
    htmlLabels: false,
    theme: dark ? 'dark' : 'neutral',
    darkMode: dark,
    // System fonts only (nothing to fetch), the same that the image is shown with.
    fontFamily: '-apple-system, BlinkMacSystemFont, "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", "Segoe UI", sans-serif',
    secure: ['secure', 'securityLevel', 'startOnLoad', 'maxTextSize', 'suppressErrorRendering', 'maxEdges',
      'htmlLabels', 'theme', 'themeCSS', 'themeVariables', 'fontFamily', 'altFontFamily', 'darkMode'],
  }
}
