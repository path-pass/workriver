# Attachments and the viewer

Files from agents and how they show and open. Files the user sends with a question are in
`conversations.md` (Asking why); pictures in a gate's form are in `activity.md` (What the
user puts in).

Moved from `DESIGN.md` in phase 7b of the restructure, word for word but for paths
made whole and "see …" naming the file; `DESIGN.md` is the overview and says where each of
its sections went.

## Attachments

- **Attachments** from agents (screenshots, reports, logs, any file) show the same way
  wherever they belong: with a task's output, a finding, or a check (next to its line).
  Images preview as thumbnails; any other file is a small chip. Pictures, Markdown and plain
  text (logs, JSON, YAML, CSV…) open in the **viewer**; any other file (and SVG) downloads.
  The store keeps every file for good, like the records.
  The viewer shows one place's previewable attachments one at a time, in their order: the
  name (mono) as its title, "2 / 5" with previous / next (← and → too, wrapping round; the
  position and name are read out), for a picture the zoom toggle and "open original",
  "Ask why", and a download button for every file. Markdown is rendered (tables, code,
  links) by react-markdown with remark-gfm, loaded only when one is shown (50 kB gzip,
  against 24 kB for marked + DOMPurify: chosen because it never puts an HTML string into
  the page, and the text is an agent's): raw HTML is dropped, links open in a new tab with
  `noopener noreferrer`, and images in it are not fetched but shown as links; a mermaid
  block is drawn as a diagram (Diagrams in Markdown, below). Plain text is
  monospace (whole JSON indented). Markdown is rendered up to its first 256 kB (about 1 s to
  render, measured), plain text up to 1 MB; past that a line says how much is shown.
  It is not modal: a large panel over the rail and the main area that stops at the side
  panel (open or folded), so the side panel stays in use; a click outside leaves it open, and
  Escape closes it unless typed in a field outside it. Focus starts in the file (↑ ↓ scroll)
  and goes back to the thumbnail or chip on close. On a narrow screen it covers the page,
  and its "Ask why" opens the Ask sheet over it; closing the sheet comes back to it.
  While it shows a stored attachment, the ask box carries it as a chip (the attachment's
  name) next to the task's; the question then points at it, and the temporary agent is given
  its path in the store (a link through the tunnel on another machine). A question may point
  only at an attachment shown with its task (its output, checks, findings and what was added
  to them, and the findings it answers – but not those for a blind task not yet delivered);
  the service refuses any other.
- **Diagrams in Markdown**: a ` ```mermaid ` block, wherever Markdown is rendered (an
  attachment, a task's input, a delivery note, an answer), is drawn as a diagram by mermaid
  (12; the format agents already write). Loaded only when a block is shown: the frame's
  script with mermaid's core, 43 kB gzip, then each kind of diagram's own chunks (a flowchart
  and a sequence diagram together 688 kB gzip, 457 kB of it ELK, mermaid 12's default layout
  for flowcharts; every kind together 1.5 MB gzip). The app's bundle grew by 301 bytes (the
  UI text) and its CSS by 221 (measured). What mermaid draws is as untrusted as the text:
  - Mermaid runs in a frame of its own, made the first time a block is shown, whose page may
    run only its own script and fetch nothing (`default-src 'none'; script-src 'self';
    style-src 'unsafe-inline'`): an image shape's URL, a stylesheet or a font a block asks
    for is refused there, and that block falls back as one mermaid cannot draw.
  - Security level `strict` (labels sanitized, a click directive calls nothing) and no HTML
    labels (every label is SVG text); a block's `%%{init}%%` cannot change these, the theme
    or the fonts. Mermaid's own `sandbox` level is not used: its iframe allows top
    navigation by a click and popups, so a `click … href` link would still open, and it does
    not stop what mermaid fetches while drawing (an image shape loads its URL with
    `new Image()` in the page).
  - The page gets the SVG text back and shows it only as an image (an `<img>` of a `data:`
    URL): an SVG image runs no script, loads nothing, and its links cannot be clicked.
  - One diagram is drawn at a time, with a pause between, so many or large ones do not hold
    the page; one no longer shown is skipped. Mermaid refuses a block over 50 000
    characters or 500 edges (its defaults).
  - Until it is drawn the block shows its code as any code block does; one mermaid cannot
    draw keeps its code, under a short note ("This diagram could not be drawn; its text is
    below."), never a broken picture or an error. The diagram is drawn in the page's theme
    (mermaid's `neutral` in light, `dark` in dark) and again when the theme changes; it keeps
    its own size, on the code block's ground, scrolled sideways when wider than the text, as
    code and tables are.
