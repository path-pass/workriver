# Presentation: colour, type, space, motion, states and loading

How everything looks and reads, whatever it shows.

Moved from `DESIGN.md` when it was split into these files, word for word but for paths
made whole and "see …" naming the file; `DESIGN.md` is the overview and says where each of
its sections went.

## Colour means one thing: the state of a piece of work

| Role | Meaning | Where |
|---|---|---|
| Water (`--water`) | It has flowed: done | Done nodes, lines that have flowed, the start |
| Amber (`--amber`) | Flowing now: someone is on it | Running nodes, the line into them, the ring around a writing agent's mark |
| Orange (`--hold`) | An agent stopped at a prompt in its terminal | The Needs-you row of an agent waiting for you in its terminal |
| Violet (`--you`) | Waiting for you, and what you picked | Gates waiting for the user, the count of findings waiting for a decision, selection outline, the file selected in the tree, focus ring, primary button |
| Neutrals | Not started, or just ground | Everything else |

One colour role is not a state: **lane colours** (`--lane-0` … `--lane-5`) only tell the
branches apart in the commit graph. They take hues none of the states use (blue, rose and
olive, each dark and light; no water, amber or violet), have light and dark values like
every token, and appear nowhere else.

Three things that used to share colour now have their own channel:

- **Who raised it** is an icon: an eye for a review, flask for tests. Every review (code,
  security, architecture) has the same eye: which kind it is stands in the node's text
  ("Security review"), so a new kind of review needs a name, not a new icon to learn.
- **What kind of work** is a shape: square corners for implementing, a pill for a review,
  a double frame with sharp corners for a gate (work only the user can do). Design work is
  drawn as implementing with a compass icon; its output is its design note, and it never
  carries the "no tests run" mark. A gate to confirm a design ("Confirm design") has a
  scale icon; its note is the decision.
- **Which agent** appears only on the agent's mark. Its colour is derived from the name
  and is used nowhere else.

Added and removed lines are green and red, the convention of every diff viewer; they
appear only in the code layer, and as words in an intent's history and in the conflict of
its edit dialog.

Colours are defined in OKLCH as semantic tokens. A theme remaps the tokens; components
never name a raw colour.

## Type

- One sans family, the system's. Monospace only for ids, paths, commit hashes and code.
- Fixed sizes, about 1.15 apart: 11, 12, 13, 14, 15, 20 px.
- Tabular numerals, so columns and line numbers align.

## Space and shape

- Spacing in multiples of 4: 4, 8, 12, 16, 20, 24.
- Radius: 6 for small controls and tree rows, 8 for nodes, 10 for blocks, fully round for
  review nodes, 3 for gates.
- Shadow appears only on hover and selection, meaning "this can be picked" or "this is
  picked".
- A chip (a file, a context, a link, a machine) is one line and never wider than what holds
  it; a list of chips wraps whole chips. A long name in it is shortened in the middle,
  keeping its start and its extension ("2026-11-16-new-acc…source.md"), and cut at the end
  only if even that does not fit; the whole name is in its tooltip and read out by screen
  readers. A long path inside running text (an answer) breaks where it must.
- Pickers (the project switch, who decides findings, a workspace link, the task a commit
  belongs to, the intent of a batch) are Radix Select in our colours, never the system's
  menu: on the rail in the rail's colours, elsewhere on the page's.

## The signature: lines are a river

- Work that is done: a solid water-coloured line.
- Work in progress: the line into it is a row of dots moving forward. Apart from the
  agent's mark this is the only looping motion, and it carries state.
- The graph has a start and no end. A finished branch simply stops; its water colour says
  it is done.
- A line that would cross columns (a secondary input reaching far) runs through the empty
  space: along the gap between rows, then down just before its target.
- With "reduce motion" set, both are still.

## States

Everything that can be picked has: default, hover, keyboard focus (violet ring), selected
(violet ring plus shadow). With a node picked, nodes and lines unrelated to it fade.

## Loading

The page must stay quick with twenty agents committing, so it reads in two parts.

- **The snapshot** (`/api/workspace`) is everything the rail, the graph and the side panel
  show, and no file text: each commit's files by path, how each changed (A, M, D) and its
  +/− lines (counted once by the service, as the diff package counts them), the uncommitted
  files the same way, and the repositories' trees (small: about 12 kB for this repository).
  Texts were nearly all of it: 8.7 MB with them, about 250 kB without.
- **A unit's code** (`/api/unit/{id}/code`: every text of its commits and of its uncommitted
  work) is read when its code layer is shown, and again only when the unit's `code` in the
  snapshot changes (a new commit, other uncommitted texts); then only the commits not read
  before (a commit never changes) and the uncommitted work come. The graph does not wait
  for it; the code layer appears once its texts are there. Read like the snapshot, so a
  blind task gets none of what its view hides.
- **Refreshing.** Each "changed" from the service reads the snapshot again, at most one read
  at a time: changes told during a read make one more read after it. A hidden tab reads
  nothing and reads once when it is shown again, unless the sound or notifications is on
  (Settings): then a hidden tab still reads on "changed", at most once every five seconds
  (`HIDDEN_READ` in `config/defaults.ts`), so a new row of Needs you sounds and notifies while
  the user is elsewhere; it reads once more when shown again only if a change came after its
  last read. A snapshot that leaves the unit on screen
  as it was keeps it, so its graph and code are not worked out and drawn again.
- The built files carry their content's hash in their names and are kept by the browser for
  good; the page itself is asked for every time. Nothing is compressed: on 127.0.0.1 gzip
  costs more time than the bytes it saves.
