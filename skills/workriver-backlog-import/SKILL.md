---
name: workriver-backlog-import
description: Import an existing backlog file (Markdown notes, a TODO list, leftover review findings, a tracker export) into workriver's backlog - planned intents and deferred findings for one project - after showing the user a preview and getting a yes. Use when the user asks to import, move or load a backlog, roadmap or list of leftover findings into workriver.
---

# Import a backlog into workriver

workriver keeps each project's backlog itself: **planned intents** (goals, in roadmap order)
and **deferred findings** (problems found earlier, with a severity, waiting to be fixed).
Your job is to turn the user's file(s) into those records. The files differ every time;
read them and use judgment, there is no fixed format.

Never edit, move or delete the source files. Never import into a project the user did not
confirm.

## 1. Read and pick the project

- Read every file the user names, in full.
- Find the target project: `workriver project list`. If the user named none, propose the
  one the file sits in. A folder with several repositories is registered by the user with
  `workriver project add <folder>`; ask before running it.
- See what is already there: `workriver backlog list --project <P>`, so you do not add
  duplicates.

## 2. Work out the items

Go through the file in order. Each item is one of:

- **Planned intent** - a goal: something to build or change. Needs a title and a
  one-sentence goal (write one from the text if the file has none).
- **Deferred finding** - a problem with a severity, usually left over from a review or a
  test run ("P2", "should fix", "Codex flagged"). Needs a severity (P1, P2 or P3) and the
  problem. P0 or "critical" becomes P1; tell the user you did that.

For both, keep:

- **notes**: the rest of the item - fix ideas ("修法", "fix:"), context, links - as the
  note. Keep links as they are.
- **source** (findings): the heading of the section the item sits in, plus anything the
  item says about where it came from (task, reviewer, branch, date), e.g.
  `"## settings-sync 遗留 / Task 1 review, opus + Codex"`.
- **external id**: a tracker key in the item (`ABC-123`, a Jira/Linear/GitHub link) as
  `--external <system>:<KEY>` and its link as `--url`.

Skip, and list as skipped: checked items (`- [x]`), sections marked done, archived,
cleared ("已清除", "archived", "resolved"), dropped items, and preamble that is not an item
(triage history, how-to text). Import them only if the user asks.

## 3. Preview and wait

Show the user, before adding anything:

- the target project (title and folder);
- one table in file order: `# | kind (intent/finding) | severity | title or problem | source | external`;
- what you will skip and why, and any severity you mapped.

Then stop and wait for an explicit yes. Apply the user's corrections and show the table
again if they changed much.

## 4. Add, in file order

One command per item, in the table's order (that order becomes the roadmap's order).
Pass multi-line notes as one quoted argument.

    workriver backlog add "<title>" --goal "<one sentence>" --project <P> [--note "<note>"] [--external <system>:<KEY> --url <link>]
    workriver backlog add-finding --project <P> --severity P1|P2|P3 --problem "<problem>" [--impact "<if left>"] [--where <path>]... [--source "<origin>"] [--note "<fix / context>"] [--external <system>:<KEY> --url <link>]

Each prints the new id (`i4`, `f7`). If one fails, stop, show the error, and ask; do not
retry with changed content on your own.

## 5. Finish

Run `workriver backlog list --project <P>` and show it, then a short list: how many intents
and findings were added, what was skipped and why. Remind the user they can reorder the
roadmap by dragging in the web UI (or `workriver backlog move <intent> --before <other>`).
