// English UI text, in i18next's format: `{{name}}` is a variable, `_one` / `_other` are
// plural forms picked by `count`. This file defines the key set (see i18next.d.ts).
export const en = {
  rail: {
    shortcut: '{{label}} (⌘B / Ctrl+B)',
    fold: 'Fold the rail',
    unfold: 'Open the rail',
    search: 'Search units',
    noMatch: 'No units match “{{q}}”',
    clearSearch: 'Clear search',
    none: 'No units recorded yet.'
  },
  manager: {
    mark: 'Manager of its intent',
    of: 'Manager {{name}}: {{state}}',
    state: { working: 'working', idle: 'idle', done: 'idle', blocked: 'blocked', unknown: 'unknown', gone: 'not running', none: 'no manager' }
  },
  service: {
    unreachable: 'Cannot reach the workriver service. Is `workriver serve` running?'
  },
  settings: {
    title: 'Settings',
    close: 'Close',
    language: 'Language',
    theme: 'Theme',
    themes: { system: 'System', light: 'Light', dark: 'Dark' }
  },
  unit: {
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
    title: 'Agents',
    idle: 'Idle'
  },
  side: {
    title: 'Needs you, agents and commits',
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
    remove: 'Remove {{name}}'
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
      unknown: 'no reason given'
    },
    failed: 'Not recorded: the service did not take it'
  },
  decide: {
    label: 'Decide on {{id}}',
    note: 'Note for the agent (optional)',
    noteShort: 'Note',
    accept: 'Accept',
    reject: 'Reject',
    defer: 'Defer',
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
    field: '{{name}}: ',
    problem: 'What is wrong',
    impact: 'If left alone',
    evidence: 'Where',
    changesOf: 'Changes from {{label}}',
    noChanges: 'No changes yet',
    noFindingsYet: 'Nothing yet.',
    noFindings: 'No problems found.',
    unknownInput: 'The commit names no task, so its input is unknown.',
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
