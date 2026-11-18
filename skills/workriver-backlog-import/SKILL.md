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
  one the file sits in. A folder with several repositories is registered by the user, not you:
  ask them to run `workriver project add <folder>` in a new terminal (not your pane; `!` in
  your session is refused too).
- See what is already there: `workriver backlog list --project <P>`, so you do not add
  duplicates.

## 2. Work out the items

Go through the file in order. Each item is one of:

- **Planned intent** - a goal the user would name themselves: usually a feature, one or a
  few related PRs. Needs a title and a one-sentence goal (write one if the file has none).
  Steps, sub-bullets and parts of one goal under the same heading or parent item are
  **one** intent: their text goes into its note as a checklist (`- [ ] …`). When unsure,
  group rather than split.
- **Deferred finding** - a problem with a severity, usually left over from a review or a
  test run ("P2", "should fix", "Codex flagged"). Needs a severity (P1, P2 or P3) and the
  problem. P0 or "critical" becomes P1; tell the user you did that.
- **Chore** - something the user themselves must do, that is neither a goal nor a problem
  in the code (renew a certificate, answer a vendor, decide a budget). Needs a title; steps
  if the file gives them.

For each, keep:

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

**Check the count** before the preview: roughly one intent per top-level goal or heading of
the file. If you have many more intents than that, regroup.

## 3. Preview and wait

Show the user, before adding anything:

- the target project (title and folder);
- one table in file order: `# | kind (intent/finding/chore) | severity | title or problem | from (its source items: "L12-18, 5 bullets") | source | external`;
- what you will skip and why, and any severity you mapped.

Then stop and wait for an explicit yes. Apply the user's corrections and show the table
again if they changed much.

## 4. Add, in file order

One command per row, in the table's order (that order becomes the roadmap's order).
Pass multi-line notes as one quoted argument.

    workriver backlog add "<title>" --goal "<one sentence>" --project <P> [--note "<note>"] [--external <system>:<KEY> --url <link>]
    workriver backlog add-finding --project <P> --severity P1|P2|P3 --problem "<problem>" [--impact "<if left>"] [--where <path>]... [--source "<origin>"] [--note "<fix / context>"] [--external <system>:<KEY> --url <link>]
    workriver chore add --project <P> --title "<what the user should do>" [--step "<step>" | --step "<step> => <command>"]...

Each prints the new id (`i4`, `f7`). If one fails, stop, show the error, and ask; do not
retry with changed content on your own.

## 5. Finish

Run `workriver backlog list --project <P>` and show it, then a short list: how many intents, chores
and findings were added, what was skipped and why. Remind the user they can reorder the
roadmap by dragging in the web UI (or `workriver backlog move <intent> --before <other>`).
If you see you split the import too finely, merge it yourself: `workriver backlog merge
<into> <from>... [--title "<new title>"]` keeps what each said in the target's note and,
with `--title`, renames the target. You may merge intents agents added (this import), not
ones the user added; those the user merges in the roadmap ("Merge…" in its heading).
