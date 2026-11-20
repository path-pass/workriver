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
  script with mermaid's core, 43 kB gzip, then each kind of diagram's own chunks (with the
  first flowchart 214 kB gzip in all, a sequence diagram 31 kB more; every kind together
  1.0 MB gzip, 3.7 MB, on disk and in the binary). Flowcharts are laid out with dagre, not
  mermaid 12's default ELK: ELK alone is 1.5 MB (457 kB gzip), so the build leaves its chunk
  out (an empty module in its place, `vite.config.ts`) and a block cannot ask for it. The
  app's bundle grew by 301 bytes (the UI text) and its CSS by 221 (measured). What mermaid
  draws is as untrusted as the text:
  - Mermaid runs in a frame of its own, made the first time a block is shown, whose page may
    run only its own script and fetch nothing (`default-src 'none'; script-src 'self';
    style-src 'unsafe-inline'`): an image shape's URL, a stylesheet or a font a block asks
    for is refused there, and that block falls back as one mermaid cannot draw.
  - Security level `strict` (labels sanitized, a click directive calls nothing) and no HTML
    labels (every label is SVG text); a block's `%%{init}%%` or front matter cannot change
    these, the layout, the theme or the fonts. Mermaid's own `sandbox` level is not used: its iframe allows top
    navigation by a click and popups, so a `click … href` link would still open, and it does
    not stop what mermaid fetches while drawing (an image shape loads its URL with
    `new Image()` in the page).
  - The page gets the SVG text back and shows it only as an image (an `<img>` of a `data:`
    URL): an SVG image runs no script, loads nothing, and its links cannot be clicked.
  - The frame shares the page's thread, and mermaid cannot be stopped once it started (a
    timer on that thread does not fire until it is done). So a block too heavy to draw is
    never handed to it: weighed from its text, without mermaid (`diagramBounds.ts`), it may
    have at most 10 000 characters, no run without white space over 1 000 (mermaid breaks
    one in time growing with its square: 732 ms for 10 000), and a weight of at most 80.
    The weight counts, statement by statement (lines, or parts between `;`), the links
    (`a & b --> c & d` as every edge it makes), or the things named besides a link's ends (a
    block diagram's row), at least one; labels, strings and text after a spaced `:` are left
    out. Mermaid's own limits (`maxTextSize`, `maxEdges`) are set to the same, and what it
    draws in place of a diagram it refuses (its error diagram, "Maximum text size in diagram
    exceeded") counts as not drawn.
  - The bound and its budget are measured (Chrome for Testing 1217, headless, on the Mac
    workriver runs on; each input in a fresh page, mermaid warmed by a two-node graph): the
    budget is that the page's 5 ms heartbeat never stops longer than 1 s. At the bound, the
    heaviest shapes found held it for 0.2–0.4 s: a class diagram of 78 relations half of them
    spanning the whole chain (357–412 ms), state and ER diagrams of that shape (286–390),
    78 such flowchart edges with labels up to 10 000 characters (306–357), `;`-joined and
    one-line chains, block rows, kanban, nested subgraphs (125–295); sequence diagrams,
    gantt, pie and the other kinds stay far under (at 200 statements: 75 ms and less). The
    cost grows with the square of edges that span many ranks: 100 of them 0.3–0.7 s, 200
    of them 1.6–2.9 s, the reviewed 499 (u15#6) 24 s, so the bound sits where the worst is
    half the budget, room for a slower machine. In the page itself (the app, then the
    first diagram, mermaid loaded then) the heaviest allowed held it for 404 ms on the
    production build; the reviewed block and one of 50 001 characters are refused and held
    it for under 10 ms.
  - One diagram is drawn at a time, with a pause between, so many do not hold the page; one
    no longer shown is skipped.
  - Until it is drawn the block shows its code as any code block does; one mermaid cannot
    draw, or too heavy to draw, keeps its code, under a short note ("This diagram could not be drawn; its text is
    below."), never a broken picture or an error. The diagram is drawn in the page's theme
    (mermaid's `neutral` in light, `dark` in dark) and again when the theme changes; it keeps
    its own size, on the code block's ground, scrolled sideways when wider than the text, as
    code and tables are.
