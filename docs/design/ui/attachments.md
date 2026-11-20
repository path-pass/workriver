# Attachments and the viewer

Files from agents and how they show and open. Files the user sends with a message to the
manager are in `conversations.md` (The Manager section); pictures in a gate's form are in `activity.md` (What the
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
  "Ask the manager", and a download button for every file. Markdown is rendered (tables, code,
  links) by react-markdown with remark-gfm, loaded only when one is shown (49 kB gzip,
  against 24 kB for marked + DOMPurify: chosen because it never puts an HTML string into
  the page, and the text is an agent's): raw HTML is dropped, links open in a new tab with
  `noopener noreferrer`, and images in it are not fetched but shown as links. Plain text is
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
  output, checks, findings and what was added to them, and the findings it answers – but
  not those for a blind task not yet delivered), or at a file the user stored; the service
  refuses any other. Agents' questions to each other follow the same rule.
