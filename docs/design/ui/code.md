# Code: the code layer and the commit graph

The main area's second layer (its first, Activity, is in `activity.md`), the Commits section
of the side panel, and the view by function that is postponed.

Moved from `DESIGN.md` when it was split into these files, word for word but for paths
made whole and "see …" naming the file; `DESIGN.md` is the overview and says where each of
its sections went.

## The code layer

  2. **Code** – the file tree on the left and the changes on the right, as in a pull
     request's "Files changed". A bar at the block's top (it stays in view while the diff
     scrolls) holds a toggle that hides or shows the tree (remembered). Where the block is
     wide (768 px or more, measured on the block, since the rails and the side panel change
     it) the tree is a pane beside the diff, as wide as its right edge is dragged (between
     220 px and half the block; remembered; a double click resets); narrower, the tree
     starts hidden and the toggle shows it as a drawer over the diff from the left (Escape
     or a press outside closes it; focus goes in and back), never squeezing the diff. The
     tree has "Filter files…" (by path; folders above a match stay; Escape clears) and its
     "only show changed" switch on top, sticks below the bar and scrolls by itself, so a
     long diff never hides it. The tree is there to keep attention on where agents are changing things:
     "only show changed" is on by default and hides every file and folder the unit did not
     touch; turned off, the whole repository shows where the changes sit. A chain of
     folders with one child each is one row. A changed file shows how it changed (A, M or
     D in the diff's green and red), its +/− lines and, while an agent writes it, the
     agent's mark with its amber ring; a folder sums its files and shows one mark (with a
     count if several write inside). The unit's change is the branch's net change against
     its base, as `git diff base...head` gives it: a file added and later moved or deleted
     on the branch, or changed and changed back, is not there, and a moved file shows only
     where it is now (and as deleted where it was, if the base has it). The base is
     counted from where the branch meets it now (`git merge-base`; the base's upstream too,
     when it is ahead), however it came in: by a merge of the base, or of another branch
     made from a newer base. Nothing the base did is the unit's. The service works this net
     change out; a merged unit's is from where it met its base just before it merged: as the
     base, or its upstream when that is what took it, stood before the merge that took it,
     from the history; only where the history cannot tell (a fast-forward) as last seen at
     the same last commit. What it is measured against follows the base's upstream as it is
     configured now, as git does, no branch having moved. Every commit is
     read against its first parent, so what a commit holds never depends on the base;
     whether a merge brought the base in (a parent other than its first is in the history of
     where they meet) does, and is worked out again whenever the base or its upstream moves; a
     merged unit's, from where it met its base then, so a branch it merged stays its work once the base has
     it too. Such a merge is no piece of work's change, even when its message names one:
     picked, that work has no change of its own; the list of commits shows nothing under it
     and its +/− are none; a line it brought, or wrote resolving a conflict, is the base's,
     from before the unit. A merge of any other branch is the branch's work. Whether a file
     changed is whether its git entry (existence, mode and object id) differs at the two
     ends, never its text, which gives only the +/− counts (uncommitted work, which git
     has not hashed, is compared by its text; a symlink's text is where it points; git's
     placeholder id is all zeros, 40 of them or, in a SHA-256 repository, 64; uncommitted
     work git lists but that cannot be read or hashed, not allowed or gone meanwhile, is a
     change, and the rest of the work is read as ever; a file's name is the one git has, line
     breaks and all, as git's NUL-separated lists give it): a file whose mode or type
     alone changed (made executable, made a symlink) is there, M with +0 −0, even when its
     text changed and changed back on the way; a mode or a submodule pointer changed and
     changed back is not. An untracked file is taken as `git add` would take it: a symlink
     with mode 120000, its text where it points (never the file there), any other as a
     regular file. A binary file (as git takes one by default: a NUL in the first 8000 bytes
     of either side) is there like any file, A, M or D, with no line counts in
     the tree or the diff, which says "Binary files differ" as git does; uncommitted, it is
     compared by the object id git would give it. Only a change's ends say whether it is
     binary: a text side keeps its text next to a binary one (a binary side's bytes are
     never sent), so a binary version between two texts, over a pick's commits or ending in
     uncommitted work, leaves the text change git shows, with its lines. A submodule's
     pointer has no text either:
     its change shows as the pointer, the commit it moved from and the one it moved to (only
     one when it is added or removed), with no line counts, never as lines, also where a
     file or a binary file became one or one became a file (git: a type change), in the
     tree, the list of commits and the single-file view alike; uncommitted, the
     pointer is the commit the submodule's checkout is at, and work inside the submodule that
     leaves it there is no change; a checkout that cannot be read (gone since git listed it)
     leaves its pointer a change to an unknown commit, said so, never git's placeholder, and
     the rest of the work is read as ever. A file the unit changes that another open unit (not
     merged, not
     closed, its intent neither done nor dropped) of the same project also changes, in the
     same repository, gets a small
     neutral mark after its name (two crossing arrows); its tooltip names those units and
     their intents ("Also changed by u2 (i1 Search)"), and the single-file view of it says the
     same in a line of its own, each unit a link that opens it. Both units' changes are
     their net change as the tree shows it with nothing picked: the net change against the
     base (`unit.net`) with uncommitted work on top, an uncommitted side compared by its text,
     as above, or, on the board (its uncommitted work read for its names only, below), by
     the id git would give it, hashed for the files its commits changed alone (a symlink's
     from where it points, its own text, as git stores it, in both reads); so nothing the
     base brought in, however it came, and nothing changed back (the service compares
     them). Uncommitted work is read again only when the service's watcher saw a change
     that touches it, and then only the files whose git entry or stat (size, modification
     and change times, inode) changed, or that the watcher named: a file in the unit's work
     tree that git does not ignore (the work tree's own lock files too, not git's), its work
     tree's own index or HEAD (a `git add`, a checkout), the ref of its branch; packed-refs
     or anything else in its repository's git folder that cannot be told, every unit of that
     repository; another branch's ref, none. A work tree is followed from the first read that
     finds it (registered with the watcher then, or tried again a minute later), and what
     was read of it before it was followed is read again once. Where the watcher cannot
     run, every read reads it all again. The board reads the names and git entries alone:
     a task's uncommitted files show their count, with +/− where the unit's code (read for
     the same code token) or an earlier read counted them, else without; the unit's code
     read reads the texts and counts. The service holds the texts of the last few units
     whose code it read (memory only), lets go of them when a unit falls out of those or is
     closed, and of a merged unit's work altogether. A file over the size limit on either
     side (512 KB: the work tree's by its stat, the committed one by its size in git) is
     never read, cat-filed or hashed: its name only, the same file and token in both reads,
     and the code shows it as not read, without +/−. The open unit's change reaches the
     page as any change: it is told, its code token changes, and its code is read again. Units on two machines are not compared: their repositories are two copies. A
     folder does not sum the mark; nothing goes to Needs you and no one is told: the user
     sees it and coordinates. A picked piece of
     work's files show its own change, even one the branch later moved; a file it added
     and deleted itself is not there either. Where other commits touched a file between the
     work's own (a merge of the base, other work), the texts do not chain: the file's change
     is shown in parts, each compared alone and headed by its commits, its +/− their sums,
     so nothing those commits did is the work's; whether it is added, changed or deleted is
     from the parts kept, so a part that undid itself says nothing. A file a piece of work plans to change shows a dashed
     circle; one it will add, and that does not exist yet, also has a dashed underline.
     With a piece of work picked, the files it touched are open and normal and the unit's
     other changes are folded and faded; with nothing picked, all of the unit's changes
     are open. Folders holding a file an agent is writing are open at first.
     With no file picked the right side lists the commits of what is picked (or of the
     whole unit), hunk by hunk. Picking a file shows that file alone and whole: every line,
     with its change over what is picked (or over the whole unit) marked, and one line
     above naming the pieces of work the change comes from. Picking it again, Escape or
     "all changes" goes back to the list; picking a node that did not touch the file lets
     it go. On a narrow screen the tree stacks above the changes with a height of its own
     and opens on the pick's files only.

     A known limit, kept for now: when the branch and its base each merged an older head of
     the other (a criss-cross), git has two equally good merge bases (`git merge-base --all`
     lists both), and the tree may start from another one than `git diff base...head` does.
     The unit's change can then show the base's files instead of the branch's, and the merge
     that brought the base in can show as that work's change.

     Also kept for now: a text file larger than 512 KiB is left out of the tree and the diff;
     binary is told by content alone, not by `.gitattributes` (`binary`, `-diff`); an
     untracked executable file is taken as 100644, not 100755.

## Commits

- **Commits** is every unit's commits as a git graph, newest first, like `git log --graph`
  or GitHub's network view: the base on the left as a plain neutral line labelled at its
  top with the base branch's name (its own commits are not drawn), one lane per unit branch
  in the unit's own tone (`--lane-0` … `--lane-5`, picked from its id; see the lane colours
  in `presentation.md`), leaving the base at the
  unit's first commit and, merged, joining it again where it was merged (the unit's
  `merged` time when known, else at its last commit); a unit closed without merging just
  ends. A lane is used again once its unit has ended, so the graph is as wide as the most
  units running side by side. One graph per repository, under its name when there are
  several. A row: the dot on its lane, the subject, quietly the unit, the task (or the red
  mark of a commit that names none) and its +/− lines, and how long ago (the date on
  hover). On top, while any agent has uncommitted work, "Writing now": the files each such
  agent is writing and its unit and task, as a hollow dashed dot on its unit's lane, dashed
  down to the unit's newest commit; the agent's mark with its amber ring is in the last
  column, where a commit's age is, so it never sits on the lanes. A row opens its unit and task and brings that
  commit into view in the code layer; it stays picked. Drawn as one small inline SVG per
  row (rows of a fixed 44 px touch, so the lines run on; 2 px lines on whole pixels stay
  crisp at 1x and 2x); no graph library.

## A view by function

- A second view of the whole repository by function, with a panel listing the units that
  changed the selected module, is postponed. The unit view is the whole app for now.
