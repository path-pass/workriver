// English UI text, in i18next's format: `{{name}}` is a variable, `_one` / `_other` are
// plural forms picked by `count`. This file defines the key set (see i18next.d.ts).
export const en = {
  rail: {
    note: 'Note',
    noteMore: 'Show more',
    noteLess: 'Show less',
    shortcut: '{{label}} (⌘B / Ctrl+B)',
    fold: 'Fold the rail',
    unfold: 'Open the rail',
    resize: 'Rail width (drag, or arrow keys; double-click for the default)',
    findings: 'Findings',
    findingsBy: { user: 'Manual', manager: 'Auto' },
    search: 'Search units',
    noMatch: 'No units match “{{q}}”',
    clearSearch: 'Clear search',
    none: 'No units recorded yet.',
    noUnits: 'No units yet',
    project: 'Project',
    allProjects: 'All projects',
    active: 'Active',
    roadmap: 'Roadmap',
    done: 'Done',
    add: 'Add',
    addTitle: 'Title',
    addGoal: 'Goal, in one sentence',
    addSave: 'Add to the roadmap',
    addCancel: 'Cancel',
    addPick: 'Pick a project above to add to its roadmap',
    drag: 'Drag to reorder',
    start: 'Start',
    intentWork: 'Research and design',
    failed: 'Not saved: {{error}}',
    mergeOpen: 'Merge…',
    mergeTitle: 'Merge planned intents',
    mergeHint: 'Tick two or more intents of one project. One stays; the others are dropped and what they said goes into its note.',
    mergeInto: 'The one that stays',
    mergeNewTitle: 'Its title',
    mergeWhat: '{{from}} will be dropped; their titles, goals, notes and links go into the note of {{into}}.',
    mergeRename: '“{{into}}” is renamed.',
    merge: 'Merge',
    held: {
      tasks: 'Has tasks of its own, so it cannot be merged (drop it with a reason instead).',
      units: 'Has units of its own, so it cannot be merged (drop it with a reason instead).',
      findings: 'Has backlog findings of its own, so it cannot be merged (drop it with a reason instead).',
      chores: 'Has chores of its own, so it cannot be merged (drop it with a reason instead).'
    }
  },
  manager: {
    mark: 'Manager of its workspace',
    of: 'Manager {{name}}: {{state}}',
    state: { working: 'working', idle: 'idle', done: 'idle', blocked: 'blocked', unknown: 'unknown', gone: 'not running', none: 'no manager' }
  },
  start: {
    title: 'Start here',
    lead: 'Nothing is recorded yet. Agents record their work themselves; one manager agent per Herdr workspace plans it.',
    agent: 'In a Herdr workspace, start a coding agent to be its manager.',
    skill: 'Give it the manager skill: link skills/workriver-manager into its skills folder, or paste what this prints.',
    register: 'Have it register as the manager of the workspace:',
    begin: 'Then it starts a goal from the roadmap on the left (or adds one first with `workriver backlog add`):',
    import: 'Have an existing backlog file? Ask an agent to import it with the import skill:',
    work: 'It splits the goal into units and tasks and hands them to worker agents; their work shows up here.',
    trailer: 'A commit belongs to a task through a line in its message:'
  },
  session: {
    title: 'workriver',
    lead: 'This page decides and acts as you, so it opens only with a link from your own terminal.',
    open: 'In a terminal that is not an agent’s pane, on the machine workriver runs on:',
    why: 'The link works once and for five minutes; after that this browser stays signed in. Agents and other machines cannot act as you.'
  },
  service: {
    unreachable: 'Cannot reach the workriver service. Is `workriver serve` running?'
  },
  settings: {
    title: 'Settings',
    close: 'Close',
    language: 'Language',
    theme: 'Theme',
    themes: { system: 'System', light: 'Light', dark: 'Dark' },
    notify: 'Notify me when an agent needs me',
    notifyDenied: 'The browser does not allow notifications for this page.'
  },
  machines: {
    title: 'Other machines',
    lead: 'Agents on other machines, followed over ssh (each machine runs its own Herdr).',
    none: 'None yet. Add one: a name and what `ssh` takes (it must work without a password prompt).',
    on: 'On {{machine}}',
    unreachable: '{{machine}} cannot be reached; shown as last seen',
    follow: 'Follow {{machine}}',
    connected_one: 'connected, {{count, number}} agent',
    connected_other: 'connected, {{count, number}} agents',
    noHerdr: 'connected, but its Herdr does not answer',
    down: 'not connected',
    connecting: 'connecting…',
    off: 'not followed',
    downSince: 'disconnected since {{time}}',
    retry_one: 'connecting again in {{count, number}} second',
    retry_other: 'connecting again in {{count, number}} seconds',
    disconnected: '{{machine}} is disconnected since {{time}}; shown as last seen',
    name: 'Name',
    ssh: 'ssh target (host or user@host)',
    add: 'Add',
    remove: 'Remove {{machine}}',
    links: 'Workspaces there',
    linkLead: 'A workspace there becomes one with a workspace here only when you link them: one manager, the same intents.',
    notLinked: 'Not linked',
    linkTo: 'Link {{workspace}} on {{machine}} to a workspace here',
    failed: 'Not done: {{error}}'
  },
  unit: {
    progress: {
      working: 'Working',
      waiting: 'Waiting',
      ready: 'Ready to merge',
      merged: 'Merged',
      done: 'Done'
    },
    complete: 'All units finished: the manager can close it',
    finished: 'Finished',
    previously: 'previously on {{branches}}',
    blocking_one: '{{count, number}} manual step an agent is waiting on',
    blocking_other: '{{count, number}} manual steps agents are waiting on',
    active: 'Active',
    untouched: 'Not started',
    waiting_one: '{{count, number}} thing waiting for you',
    waiting_other: '{{count, number}} things waiting for you',
    grew_other: '{{count, number}} beyond plan',
    grewHint: 'A branch grew beyond the original plan'
  },
  agents: {
    blocked: 'Waiting for you in its terminal',
    notice: '{{agent}} is waiting for you',
    title: 'Agents',
    idle: 'Idle'
  },
  plan: {
    title: 'Start “{{title}}”',
    lead: 'workriver does not start agents. These are the steps, in Herdr:',
    workspace: 'Open a Herdr workspace for it, or pick one whose manager has room for another intent.',
    agent: 'If the workspace has no manager yet, start a coding agent there and give it the manager skill:',
    register: 'In the manager’s pane, register it:',
    start: 'Then start this intent there; new units and tasks in that workspace go to it:'
  },
  backlog: {
    title: 'Backlog',
    lead: 'Deferred findings, waiting to be done.',
    none: 'Nothing deferred here.',
    every: 'Every project; pick one in the rail to narrow.',
    together: 'Do together',
    drop: 'Drop',
    dropReason: 'Why drop them?',
    dropConfirm_one: 'Drop {{count, number}}',
    dropConfirm_other: 'Drop {{count, number}}',
    cancel: 'Cancel',
    selected_one: '{{count, number}} selected',
    selected_other: '{{count, number}} selected',
    for: 'For',
    startFirst: 'Start {{intent}} first: its manager runs `workriver intent start {{intent}}`.',
    oneIntent: 'Pick findings of one intent.',
    noIntent: 'No active intent in this project to do them in.',
    sent: 'Sent to {{to}} as one message.',
    scheduled: 'scheduled',
    fromSource: 'From {{source}}',
    added: 'Added to the backlog',
    missing: 'no impact or place given',
    unit: '{{intent}} · {{unit}}'
  },
  side: {
    title: 'Needs you, agents, commits, backlog and ask why',
    close: 'Fold the panel'
  },
  terminal: {
    open: 'Open in terminal',
    openFor: 'Open {{agent}} in terminal',
    note: 'Would bring {{agent}}’s terminal to the front – not connected yet'
  },
  evidence: {
    label: 'Checks run',
    passed: 'Passed',
    failed: 'Failed',
    none: 'No tests run'
  },
  ask: {
    placeholderGate: 'Ask {{agent}} about this step…',
    placeholderManager: 'Ask the manager bot…',
    why: 'Ask why',
    label: 'Question about this work',
    placeholder: 'Ask {{who}}’s agent why…',
    send: 'Send',
    remove: 'Leave {{label}} out',
    how: 'A temporary agent answers from the records and the code; it asks {{agent}} only when {{agent}} is idle.',
    howAlone: 'A temporary agent answers from the records and the code.',
    step: {
      starting: 'Starting an agent…',
      reading: 'Reading the records and the code…',
      asking: 'Asking {{agent}}…'
    },
    asked: 'Asked {{agent}}',
    busy: '{{agent}} was busy; answered from the records',
    ended: '{{agent}}’s session has ended; answered from the records',
    cannot: 'Could not start an agent to answer: {{reason}}',
    end: 'End',
    endHint: 'End this conversation; its agent closes',
    threadEnded: 'Ended',
    conclude: 'Draft conclusion',
    concludeAsked: 'Conclusion asked for',
    conclusion: 'Conclusion',
    fresh: 'New question',
    exchanges_one: '{{n}} exchange',
    exchanges_other: '{{n}} exchanges',
    earlier_one: '{{count, number}} earlier conversation',
    earlier_other: '{{count, number}} earlier conversations',
    state: {
      answered: 'answered',
      waiting: 'waiting',
      failed: 'failed',
      ended: 'ended'
    },
    empty: 'Pick a task in the graph, or lines in the code, to ask why about it.',
    conversations_one: '{{count, number}} conversation',
    conversations_other: '{{count, number}} conversations',
    count_one: '{{count, number}} question asked',
    count_other: '{{count, number}} questions asked'
  },
  chore: {
    done: 'Done'
  },
  commits: {
    title: 'Commits',
    writing: 'Writing now',
    today: 'Today',
    yesterday: 'Yesterday'
  },
  inbox: {
    title: 'Needs you',
    blocking: 'Blocking',
    blockedAgent: '{{agent}} is waiting for you in its terminal',
    decisions: 'Decisions',
    gaps: 'Gaps',
    later: 'Not urgent',
    empty: 'Nothing needs you.',
    suggested_one: '{{count, number}} with an agent suggestion',
    suggested_other: '{{count, number}} with an agent suggestion'
  },
  need: {
    loose: 'Unregistered',
    far: 'Grew beyond the plan'
  },
  kind: {
    implement: 'Implement',
    design: 'Design',
    research: 'Research',
    gate: 'Manual step',
    review: 'Review'
  },
  state: {
    todo: 'Up next',
    running: 'In progress',
    done: 'Done',
    waiting: 'Waiting for you',
    unregistered: 'Unregistered'
  },
  reviewer: {
    'security-review': 'Security review',
    'code-review': 'Code review',
    test: 'Tests'
  },
  gate: {
    deploy: 'Manual deploy',
    test: 'Manual test',
    decide: 'Confirm design',
    result: 'Result',
    done: 'Done',
    problem: 'Problem',
    note: 'Note (paste screenshots here)',
    finding: 'Finding {{n}}',
    severity: 'Severity',
    where: 'Where: file paths, comma or line separated',
    addFinding: 'Add a finding',
    removeFinding: 'Remove this finding',
    needFinding: 'A problem needs at least one finding with what is wrong.',
    submit: 'Submit'
  },
  attach: {
    add: 'Attach',
    hint: 'or paste / drop images',
    open: 'Show {{name}} large',
    remove: 'Remove {{name}}',
    zoomIn: 'Full width (click the picture too)',
    zoomOut: 'Whole picture (click the picture too)',
    original: 'Open the original in a new tab'
  },
  deliver: {
    pending: 'Not delivered yet',
    delivered: 'Delivered to {{agent}}',
    toManager: 'Sent to the manager bot',
    picked: '{{agent}} picked it up',
    undo: 'Undo',
    copy: 'Copy message',
    recorded: 'Recorded',
    notDelivered: 'Not delivered: {{reason}}',
    reason: {
      'no-manager': 'the intent has no manager',
      'manager-gone': 'the manager is not running',
      'runtime-down': 'Herdr is not reachable',
      'runtime-error': 'Herdr refused it',
      'no-workspace': 'its intent has no Herdr workspace',
      timeout: 'no answer in time',
      restarted: 'the service restarted',
      unknown: 'no reason given'
    },
    failed: 'Not recorded: the service did not take it'
  },
  decide: {
    label: 'Decide on {{id}}',
    note: 'Note for the agent (optional)',
    noteShort: 'Note',
    accept: 'Accept',
    revise: 'Revise',
    reject: 'Reject',
    defer: 'Defer',
    reviseNeedsNote: 'Revise needs a note saying how it is to be fixed.',
    suggested: '(the agent suggests this)'
  },
  steps: {
    copy: 'Copy command',
    copied: 'Copied'
  },
  review: {
    found_one: '{{by}}: {{count, number}} finding',
    found_other: '{{by}}: {{count, number}} findings',
    passed: '{{by}}: no problems',
    running: '{{by}}: in progress'
  },
  finding: {
    open: 'Waiting for you',
    rejected: 'Rejected',
    deferred: 'Deferred',
    managerDecides: 'The manager decides',
    noManager: 'No manager running: yours to decide',
    notes_one: '{{count, number}} addition',
    notes_other: '{{count, number}} additions',
    noted: '{{by}} added',
    override: 'Decided by the manager. You can decide differently:',
    scheduled: 'Scheduled',
    done: 'Done',
    dropped: 'Dropped',
    revised: 'Revise',
    acceptedHere: 'Accepted',
    suggests: 'Suggests {{decision}}',
    decision: { accept: 'accept', reject: 'reject', defer: 'defer' },
    byAgent: 'Decided by the agent',
    leadsTo: 'Leads to',
    causes: 'Findings',
    details: 'Details of {{id}}',
    waiting_one: '{{count, number}} finding waiting for you',
    waiting_other: '{{count, number}} findings waiting for you'
  },
  task: {
    blind: 'Blind',
    blindHint: 'Blind: done without seeing other work, so its findings are its own view',
    unregistered: 'Unregistered: {{title}}'
  },
  who: {
    writing: '{{name}}, writing now'
  },
  flow: {
    empty: 'No tasks recorded for this unit yet.',
    start: 'Start',
    controls: 'Graph controls',
    zoomIn: 'Zoom in',
    zoomOut: 'Zoom out',
    fit: 'Fit the whole graph'
  },
  io: {
    input: 'Input',
    output: 'Output',
    amended: 'amended {{when}}',
    field: '{{name}}: ',
    problem: 'What is wrong',
    impact: 'If left alone',
    evidence: 'Where',
    changesOf: 'Changes from {{label}}',
    noChanges: 'No changes yet',
    noFindingsYet: 'Nothing yet.',
    noFindings: 'No problems found.',
    unknownInput: 'The commit names no task, so its input is unknown.',
    linkTo: 'It belongs to task',
    linked: 'linked by hand',
    rebasedFrom: 'rebased from {{hash}}',
    lostCommits: 'Its commits are not on the current branch: {{commits}} (on a branch this unit left)',
    uncommitted: 'Writing, not committed yet',
    notStarted: 'Not started yet.',
    notDone: 'Not done yet.',
    willChange: 'Expected to change: {{paths}}'
  },
  stat: {
    files_one: '{{count, number}} file',
    files_other: '{{count, number}} files'
  },
  list: {
    sep: ', '
  },
  legend: {
    done: 'Done',
    running: 'In progress',
    todo: 'Not started',
    waiting: 'Findings waiting for you',
    flowed: 'Flowed',
    flowing: 'Flowing now',
    also: 'Secondary input'
  },
  block: {
    activity: 'Activity',
    code: 'Code'
  },
  file: {
    back: 'All changes',
    from: 'Changed by',
    unchanged: 'This unit has not changed {{path}} yet.'
  },
  tree: {
    label: 'Files',
    onlyChanged: 'Only show changed',
    filter: 'Filter files…',
    noMatch: 'No file matches',
    collapse: 'Collapse file tree',
    expand: 'Expand file tree',
    resize: 'Width of the file tree',
    planned: 'Planned, not changed yet',
    ghost: 'Planned, not there yet'
  },
  change: {
    A: 'Added',
    M: 'Modified',
    D: 'Deleted'
  },
  code: {
    none: 'No changes.',
    moreFiles_one: '{{count, number}} more file',
    moreFiles_other: '{{count, number}} more files'
  }
}
