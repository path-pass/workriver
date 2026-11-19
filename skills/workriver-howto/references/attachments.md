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

**Refusals:** "… is larger than 50 MB". On a blind task you can open only what you attached
yourself.
