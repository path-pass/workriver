# Two tasks that must fit together

**When:** front and back end, or any two tasks built in parallel against one interface.

    workriver task add u1 --kind design --title "API contract for mentions" --agent designer
    workriver task add u1 --title "Mentions API" --after 1 --agent back --text "Contract: docs/mentions-api.md"
    workriver task add u1 --title "Mentions UI" --after 1 --agent front --text "Contract: docs/mentions-api.md"

The design task writes the contract as a file in the repository (types, API, props). If the
user must choose, add a decide gate (`design-gate`).

Questions between the two go through `workriver ask`. A change to the contract is not
agreed between the workers: the one who needs it records a review of the design task with
one finding (file format: `finding-additions`):

    workriver task add u1 --kind review --after 1
    workriver task start u1#4
    workriver task deliver u1#4 --findings contract-change.json

**You will see:** "recorded findings P2-1"; it is decided like any finding
(`deciding-findings`), and an accepted one leads to a revised design.
