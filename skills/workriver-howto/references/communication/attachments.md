# Screenshots and reports

**When:** a check or a finding has a file that shows it. Files go through workriver, never
into the repository.

    workriver attach --dir u1#3                              # the task's folder: save files there
    workriver task deliver u1#3 --evidence 'npx playwright test => failed: 1 failed => report.html'
    workriver attach shot.png --task u1#3 [--finding P2-1] [--move]

Files in the task's folder are attached when you deliver. One named after a finding
(`P2-1-login.png`) goes with that finding. In findings JSON: `"attachments": ["/abs/path.png"]`.

**You will see:** messages about the task list the stored paths; on another machine, the
command that fetches each file into the folder you are in (`workriver attach fetch
<id>.<ext>`, as printed; a `curl` to the service is refused, it shows no process).

A file someone else must read is attached to a task, or named in its text (its path, or
the fetch command): what open work names that way is kept while the work is open.

**Gone files:** the user may set a rule that deletes or moves files some days after their
work is closed. The task still says what each was: a deleted one is shown as `name
(removed 2026-12-20)`, a moved one by `workriver attach fetch <id>.<ext>` (the service
reads it where it went and checks it), and `task show` lists files its text names that
went ("Named in the text, no longer in the store: …"). Fetching a deleted one answers
"… was removed on … (the attachment rule)". Do not ask for it again unless the work needs it.

**Refusals:** "… is larger than 50 MB". On a blind task you can open only what you attached
yourself.
