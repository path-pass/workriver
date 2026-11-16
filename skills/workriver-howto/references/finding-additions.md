# Adding to findings after a review was delivered

**When:** a reviewer has a requirement for the fix of a decided finding, a detail, a file;
or more findings. A finding is never rewritten.

    workriver finding note u1#4 P1-1 --text "The fix needs a test with an expired token" [--attach trace.txt]
    workriver task deliver u1#4 --amend --add-findings more.json

`more.json`: `[{"severity":"P2","problem":"…","impact":"…","where":["src/a.rs"]}]`

A note is by the agent that delivered the task, the intent's manager, or the user.

**You will see:** `recorded` / "added findings P1-2" (numbered after those there). Agents of
open work the finding led to, and the manager, are told; `workriver board` lists the
addition under the cause.

A fix that does not satisfy a decided finding is a **new review task of the fix**
(`workriver task add u1 --kind review --after <fix>`), not an edit.

**Refusals:** "some findings … are already decided, so they cannot be replaced" - use
`--add-findings`. "a finding needs a problem, an impact and at least one where".
