# Working with workriver

Placeholder: the full instructions come later. Record your work from your shell:

- `workriver task start <unit>#<n>` when you begin.
- Commit with the trailer line `Task: <unit>#<n>` in every commit message.
- `workriver task deliver <unit>#<n> --evidence 'COMMAND => passed: SUMMARY'` when done;
  reviews add `--findings FILE` (JSON; see `workriver task deliver --help`).
- `workriver --help` explains the rest.
