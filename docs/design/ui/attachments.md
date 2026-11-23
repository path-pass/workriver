# Attachments and the viewer

Files from agents and how they show and open. Files the user sends with a message to the
manager are in `conversations.md` (The Manager section); pictures in a gate's form are in `activity.md` (What the
user puts in).

Moved from `DESIGN.md` when it was split into these files, word for word but for paths
made whole and "see …" naming the file; `DESIGN.md` is the overview and says where each of
its sections went.

## Attachments

- **Attachments** from agents (screenshots, reports, logs, any file) show the same way
  wherever they belong: with a task's output, a finding, or a check (next to its line).
  Images preview as thumbnails; any other file is a small chip. Pictures, Markdown and plain
  text (logs, JSON, YAML, CSV…) open in the **viewer**; any other file (and SVG) downloads.
  The store keeps every file for good, like the records, unless the user sets an
  attachment rule (The attachment rule, below).
  The viewer shows one place's previewable attachments one at a time, in their order: the
  name (mono) as its title, "2 / 5" with previous / next (← and → too, wrapping round; the
  position and name are read out), for a picture the zoom toggle and "open original",
  "Ask the manager", and a download button for every file. Markdown is rendered (tables, code,
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
  and its "Ask the manager" opens the Manager sheet over it; closing the sheet comes back to it.
  While it shows a stored attachment, the Manager box carries it as a chip (the attachment's
  name) next to the task's; the message then points at it, and the manager is given its
  path in the store (a command that fetches it, on another machine). A message may point
  only at an attachment shown with its task (with the unit alone, with one of its tasks: its
  output, checks, findings and what was added to them, what was added to what it asks, and
  the findings it answers – but not those for a blind task not yet delivered), or at a file
  the user stored; the service
  refuses any other. Agents' questions to each other follow the same rule.
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
    labels (every label is SVG text). A block changes none of mermaid's settings: before
    mermaid is given it, its front matter and its `%%{…}%%` directives are taken away (one not
    closed to the end of the text, as mermaid reads it; a front matter's title goes too).
    `secure` alone does not hold: it names keys at the top level only, and mermaid also reads
    a nested name written there (a top-level `rankSpacing` of 1 000 000 drew an image
    2 000 142 px tall, as a review found); it stays as a second line. With every size and spacing
    each drawn kind reads written at the top level, in a directive or in front matter, each
    image keeps the size it has without them. Mermaid's own `sandbox` level is not used: its iframe allows top
    navigation by a click and popups, so a `click … href` link would still open, and it does
    not stop what mermaid fetches while drawing (an image shape loads its URL with
    `new Image()` in the page).
  - The page gets the SVG text back and shows it only as an image (an `<img>` of a `data:`
    URL): an SVG image runs no script, loads nothing, and its links cannot be clicked.
  - The frame shares the page's thread, and mermaid cannot be stopped once it started (a
    timer on that thread does not fire until it is done). So a block is drawn only when its
    text, read without mermaid (`content/diagram/bounds.ts`), shows it cheap enough; any
    other is never handed to mermaid:
    - **Its kind is one drawn**: flowchart (`graph`, `flowchart`), sequence, class, state and
      ER diagrams, by the first word after front matter, directives and comments; the frame
      checks again with mermaid's own `detectType`. Every other kind shows its code: gantt,
      block, packet and some others take numbers that make work no count of statements sees
      (as a review found: a block diagram's `space:20000` held the page 19–26 s, a gantt chart of 70
      tasks of 12 000 days with `excludes weekends` 4.7–6.8 s, a packet's `0-3999999` 0.5–1.8
      s), and the rest were not measured to the bound.
    - **Its size is within the bounds**: at most 10 000 characters, no run without white space
      over 1 000 (mermaid breaks one in time growing with its square: 732 ms for 10 000), and a
      weight of at most 80. The weight counts, statement by statement (lines, or parts between
      `;`), the links (`a & b --> c & d` as every edge it makes), or the things named besides
      a link's ends, at least one; labels, strings and text after a spaced `:` are left out.
      A link's length (`A ------> B`) costs nothing measurable (10 000 characters of them: 144
      ms at most). Mermaid's own limits (`maxTextSize`, `maxEdges`) are set to the same, and
      what it draws in place of a diagram it refuses (its error diagram, "Maximum text size in
      diagram exceeded") counts as not drawn.
  - The kinds and bounds are measured (Chrome for Testing 1217, headless, on the Mac
    workriver runs on; each input in a fresh page, mermaid warmed by a two-node graph, three
    runs): the budget is that the page's 5 ms heartbeat never stops longer than 1 s. At the
    bounds, the worst shape found of each kind drawn, longest stop of the runs (the last measurement;
    the same shapes in an earlier one, on a busier machine, in brackets):
    - flowchart: 320 ms, 78 edges half of them spanning the whole chain, labels up to
      10 000 characters (306–357); the same without labels, as lines, one-line chains or
      `;`-joined: 205–216 (210–264); random and complete graphs, nested subgraphs, long
      links, labels filling 10 000 characters on 80 nodes: 74–203;
    - ER: 254 ms, 78 relations of that shape (300–350); entities of 8 attributes: 35;
    - class: 244 ms, 78 relations of that shape (357–412); namespaces, classes of 8
      members: 30–55;
    - state: 221 ms, 78 transitions of that shape (286–390); composite states nested 14
      deep: 67;
    - sequence: 70 ms, 78 activations; messages, notes, blocks nested 18 deep, messages
      filling 10 000 characters: 33–46.
    The cost of the graph kinds grows with the square of edges that span many ranks: 100 of
    them 0.3–0.7 s, 200 of them 1.6–2.9 s, the 499 of a review's input 24 s, so the weight bound
    sits where the worst is under half the budget, room for a slower machine. In the page
    itself (the app, then the first diagram, mermaid loaded then; production build) the
    heaviest allowed held it for 364 ms (404 in the earlier measurement); every refused block, the
    inputs reviews found too heavy among them, for under 15 ms.
  - One diagram is drawn at a time, with a pause between, so many do not hold the page; one
    no longer shown is skipped.
  - Until it is drawn the block shows its code as any code block does; one mermaid cannot
    draw, or one not drawn here, keeps its code, under a short note ("This diagram could not be drawn; its text is
    below."), never a broken picture or an error. The diagram is drawn in the page's theme
    (mermaid's `neutral` in light, `dark` in dark) and again when the theme changes; it keeps
    its own size, on the code block's ground, scrolled sideways when wider than the text, as
    code and tables are.

## The attachment rule

The service's side, which the page reads and sets (the Settings tab "Attachments" and the
marks on a file that went are the page's, written with them). One rule, global, kept in
`settings.json` under `attachments` and never as an event; `docs/security.md` says who may
do what.

- **The rule**: `{ after: "keep" | "delete" | "move", days: 1–3650 (30), only: "all" |
  "pictures", minMb: 0–50000 (0: any size, an empty file too), target?: { kind:
  "folder", path } }`. `keep` is the default and is not written while it is. `target` is
  read only for `move`; the folder must be absolute, exist, be a folder, be neither the
  data directory, the store's folder or its inbox (each followed through its links) nor
  inside one of them (by what the folders are, not by the spelling of their paths), and be
  writable; it is saved canonical, and the service never makes it. A rule
  out of range in the file is never brought into range: it runs nothing and says why.
- **When a file goes**: once every record that names it (as an attachment, or by its id in
  any text) is closed: its intent done or dropped, its workspace with no active intent, a
  chore done, a question answered, a backlog finding dropped; then `days` after the latest
  of those closes and the last time the file was stored or named. A file of an active
  intent never goes. Under `move`, a file with no bytes in the store is skipped and
  counted in the run's `words`, never recorded as gone (a file never stored and a disk not
  mounted at the store look the same); under `delete` it is recorded as deleted.
- **`GET /api/settings/attachments`** → `{ rule, error, usage: { files, bytes }, lastRun,
  running, next }`. `rule` is null and `error` says why when the file's rule is out of
  range. `lastRun` is the last run since the service started (null before one): `{ at, by:
  "timer" | "user", ms, ok, words, detail: { how: "deleted" | "moved" | null, files, bytes,
  failed } }`; `words` is one English line with counts only. `next` is when the timer runs
  it next.
- **`POST /api/settings/attachments`** `{ rule, dryRun? }` → `{ rule, due: { files, bytes }
  }`: the rule as saved (its folder canonical) and what it would take now. With `dryRun`
  nothing is saved. A refused rule or folder answers 422 with the reason in words.
- **`POST /api/settings/attachments/run`** → the run's result, the shape of `lastRun`; 409
  while a run is on (its result shows when it ends); 404 on a service that does not run
  the rule. (Not under `/api/attachments/…`, where a machine's listener would take `run`
  for an attachment's id.) The service also runs it a minute after it starts, then every
  hour.
- All three are the user's (`App::user`) and none is on a machine's listener.
- **A file that went**: an attachment in the page's JSON has `gone?: { how: "deleted" |
  "moved", at, to? }`, `to` the target as recorded with the move (`{ kind, … }`; the page
  shows a folder's `path`, and the kind's name for a kind it does not know).
  `GET /api/attachments/{id}` serves a moved file from its folder, only if its size and
  content are what was stored; a moved one that cannot be read there is 404 in words; a
  deleted one is 410. Agents are given a moved file's fetch command, never a path into the
  folder.
