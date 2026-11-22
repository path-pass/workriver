# After workriver was updated

**When:** you got "[workriver] workriver was updated; run `workriver guide changes`", a
command you know is refused, or a command in a recipe is unknown.

    workriver status                          # the service's version and commit; warns if this CLI is another build
    workriver guide changes                   # what behaves differently now
    workriver guide manager                   # the role, as the installed version means it
    workriver guide how                       # the recipes

The text `workriver guide …` prints always matches the installed binary; a copied skill
file may be older. Each machine has its own `workriver` binary: on another machine,
`workriver status` there shows whether it is the same build as the service.

Rebuilding is the user's: `git pull`, build, restart `workriver serve`; the same build on
every machine followed.

After the update that made workspaces workriver's own, nobody is a manager until the user
seats them again: "only a workspace's manager … starts an intent" or "only a workspace's
manager says something to the user" from a pane that managed before means that. Tell the
user which workspace you managed; they seat you with one click (`seat`).

**Refusals:** "unrecognized subcommand" - this CLI is older than the recipe: tell the user
the builds differ.
