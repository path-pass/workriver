// Chinese UI text. Typed against en.ts so a missing or extra key is a compile error.
// Chinese has one plural form, so the `_one` keys are left out.
import type { en, OneForm } from './en'

export const zhCN: OneForm<typeof en> = {
  rail: {
    shortcut: '{{label}}（⌘B / Ctrl+B）',
    fold: '收起侧栏',
    unfold: '展开侧栏',
    resize: '侧栏宽度（拖动，或用方向键；双击恢复默认）',
    findings: '发现由谁决定',
    findingsBy: { user: '只由我', manager: '管理者' },
    search: '搜索单元',
    noMatch: '没有单元匹配「{{q}}」',
    clearSearch: '清除搜索',
    none: '还没有记录任何单元。',
    noUnits: '还没有单元',
    project: '项目',
    allProjects: '全部项目',
    active: '进行中',
    roadmap: '路线图',
    done: '已完成',
    add: '添加',
    addTitle: '标题',
    addGoal: '目标，一句话',
    addSave: '加入路线图',
    addCancel: '取消',
    addPick: '先在上面选一个项目，才能加到它的路线图',
    drag: '拖动调整顺序',
    start: '开始',
    intentWork: '调研与设计',
    failed: '没有保存：{{error}}',
    mergePick: '选中以合并：{{title}}',
    mergeInto: '合并到',
    mergeWhat: '{{from}} 会被放弃；它们的标题、目标、备注和链接写进 {{into}} 的备注。',
    merge: '合并',
    mergeOneProject: '只能合并同一个项目的目标。'
  },
  manager: {
    mark: '这个工作区的管理者',
    of: '管理者 {{name}}：{{state}}',
    state: { working: '工作中', idle: '空闲', done: '空闲', blocked: '卡住了', unknown: '状态不明', gone: '没在运行', none: '没有管理者' }
  },
  start: {
    title: '从这里开始',
    lead: '还没有任何记录。agent 自己记录自己的工作；每个 Herdr 工作区有一个管理者 agent 负责规划。',
    agent: '在一个 Herdr 工作区里，启动一个 coding agent 当它的管理者。',
    skill: '把管理者 skill 给它：把 skills/workriver-manager 链接进它的 skills 目录，或者把下面这条命令的输出贴给它。',
    register: '让它登记为这个工作区的管理者：',
    begin: '然后它从左边的路线图里开始一个目标（或者先用 `workriver backlog add` 加一个）：',
    import: '已经有一份待办文件？让一个 agent 用导入 skill 把它导进来：',
    work: '它会把目标拆成单元和任务，交给 worker agent；它们的工作会显示在这里。',
    trailer: '提交信息里写这样一行，这个提交就归到对应的任务：'
  },
  session: {
    title: 'workriver',
    lead: '这个页面替你做决定、以你的身份操作，所以只能用你自己终端里给出的链接打开。',
    open: '在 workriver 所在的机器上，找一个不是 agent 窗格的终端运行：',
    why: '这个链接只能用一次，五分钟内有效；之后这个浏览器会保持登录。agent 和其他机器不能以你的身份操作。'
  },
  service: {
    unreachable: '连不上 workriver 服务。`workriver serve` 在运行吗？'
  },
  settings: {
    title: '设置',
    close: '关闭',
    language: '语言',
    theme: '主题',
    themes: { system: '跟随系统', light: '浅色', dark: '深色' },
    notify: 'agent 需要我时通知我',
    notifyDenied: '浏览器不允许这个页面发通知。'
  },
  machines: {
    title: '其他机器',
    lead: '其他机器上的 agent，通过 ssh 跟踪（每台机器跑自己的 Herdr）。',
    none: '还没有。加一台：一个名字，加上 `ssh` 能用的目标（必须不用输密码就能连上）。',
    on: '在 {{machine}} 上',
    unreachable: '连不上 {{machine}}；显示的是上次看到的状态',
    follow: '跟踪 {{machine}}',
    connected_other: '已连接，{{count, number}} 个 agent',
    noHerdr: '已连接，但那边的 Herdr 没有应答',
    down: '没连上',
    connecting: '正在连接…',
    off: '未跟踪',
    downSince: '{{time}} 起断开',
    retry_other: '{{count, number}} 秒后重连',
    disconnected: '{{machine}} 从 {{time}} 起断开；显示的是上次看到的状态',
    name: '名字',
    ssh: 'ssh 目标（主机名或 user@host）',
    add: '添加',
    remove: '移除 {{machine}}',
    links: '那边的工作区',
    linkLead: '那边的工作区只有在你把它和这边的工作区关联之后才算同一个：一个管理者，同一批意图。',
    notLinked: '未关联',
    linkTo: '把 {{machine}} 上的 {{workspace}} 关联到这边的一个工作区',
    failed: '没有完成：{{error}}'
  },
  unit: {
    progress: {
      working: '进行中',
      waiting: '等待中',
      ready: '可以合并',
      merged: '已合并',
      done: '已结束'
    },
    complete: '所有单元都已完成：管理者可以结束这个意图',
    finished: '已完成',
    previously: '之前在 {{branches}}',
    blocking_other: '{{count, number}} 个手动步骤卡住了 agent',
    active: '进行中',
    untouched: '没动手',
    waiting_other: '{{count, number}} 件事等你',
    grew_other: '长出原计划 {{count, number}}',
    grewHint: '有分支长到了原计划之外'
  },
  agents: {
    blocked: '在终端里等你',
    notice: '{{agent}} 在等你',
    title: 'Agent',
    idle: '空闲'
  },
  plan: {
    title: '开始「{{title}}」',
    lead: 'workriver 不会启动 agent。在 Herdr 里按这几步做：',
    workspace: '为它开一个 Herdr 工作区，或者选一个管理者还能再管一个意图的工作区。',
    agent: '如果这个工作区还没有管理者，在里面启动一个 coding agent，并把管理者 skill 给它：',
    register: '在管理者的窗格里登记：',
    start: '然后在那里开始这个意图；这个工作区里新建的单元和任务都归它：'
  },
  backlog: {
    title: '待办',
    lead: '延后的发现，等着去做。',
    none: '这里没有延后的发现。',
    every: '显示全部项目；在左栏选一个项目可以缩小范围。',
    together: '一起做',
    drop: '放弃',
    dropReason: '为什么放弃？',
    dropConfirm_other: '放弃 {{count, number}} 条',
    cancel: '取消',
    selected_other: '已选 {{count, number}} 条',
    for: '归入',
    startFirst: '先开始 {{intent}}：由它的管理者运行 `workriver intent start {{intent}}`。',
    oneIntent: '请只选同一个意图的发现。',
    noIntent: '这个项目里没有进行中的意图可以做它们。',
    sent: '已作为一条消息发给 {{to}}。',
    scheduled: '已排期',
    fromSource: '来自 {{source}}',
    added: '直接加入待办',
    missing: '没有写影响或位置',
    unit: '{{intent}} · {{unit}}'
  },
  side: {
    title: '需要你、Agent、提交和待办',
    close: '收起面板'
  },
  terminal: {
    open: '在终端里打开',
    openFor: '在终端里打开 {{agent}}',
    note: '将把 {{agent}} 的终端切到前台（还没有接上）'
  },
  evidence: {
    label: '跑过的检查',
    passed: '通过',
    failed: '失败',
    none: '没有跑过测试'
  },
  ask: {
    placeholderGate: '问 {{agent}} 关于这一步…',
    placeholderManager: '问管理 bot…',
    why: '问为什么',
    label: '关于这项工作的问题',
    placeholder: '问 {{who}} 的 agent 为什么…',
    send: '发送',
    remove: '不带上 {{label}}',
    how: '由临时 agent 根据记录和代码回答；{{agent}} 空闲时才会去问它。',
    howAlone: '由临时 agent 根据记录和代码回答。',
    step: {
      starting: '正在启动 agent…',
      reading: '正在读记录和代码…',
      asking: '正在问 {{agent}}…'
    },
    asked: '问过 {{agent}}',
    busy: '{{agent}} 正忙，根据记录回答',
    ended: '{{agent}} 的会话已结束，根据记录回答',
    cannot: '没能启动回答的 agent：{{reason}}',
    end: '结束',
    endHint: '结束这段对话；它的 agent 会关掉',
    threadEnded: '已结束',
    conclude: '起草结论',
    concludeAsked: '请它写结论',
    conclusion: '结论',
    exchanges_other: '{{n}} 轮',
    earlier_other: '更早的 {{count, number}} 段对话',
    state: {
      answered: '已回答',
      waiting: '等待中',
      failed: '失败',
      ended: '已结束'
    },
    fresh: '新问题',
    count_other: '{{count, number}} 个提问'
  },
  chore: {
    done: '做完了'
  },
  commits: {
    title: '提交',
    writing: '正在写',
    today: '今天',
    yesterday: '昨天'
  },
  inbox: {
    title: '需要你',
    blocking: '卡住了',
    blockedAgent: '{{agent}} 在终端里等你',
    decisions: '待决定',
    gaps: '缺口',
    later: '不急',
    empty: '没有需要你处理的事。',
    suggested_other: '{{count, number}} 条有 agent 建议'
  },
  need: {
    loose: '未登记',
    far: '长出了原计划'
  },
  kind: {
    implement: '实现',
    design: '设计',
    research: '调研',
    gate: '要你动手',
    review: '审查'
  },
  state: {
    todo: '接下来',
    running: '正在做',
    done: '已完成',
    waiting: '等你',
    unregistered: '未登记'
  },
  reviewer: {
    'security-review': '安全审查',
    'code-review': '代码 review',
    test: '测试'
  },
  gate: {
    deploy: '手动部署',
    test: '手动试用',
    decide: '确认方案',
    result: '结果',
    done: '做完了',
    problem: '有问题',
    note: '备注（截图可以直接粘贴到这里）',
    finding: '第 {{n}} 个问题',
    severity: '严重程度',
    where: '出处：文件路径，用逗号或换行分开',
    addFinding: '再加一个问题',
    removeFinding: '删掉这个问题',
    needFinding: '有问题时至少要写一个问题，说清哪里不对。',
    submit: '提交'
  },
  attach: {
    add: '添加图片',
    hint: '也可以粘贴或拖进来',
    open: '放大查看 {{name}}',
    remove: '移除 {{name}}'
  },
  deliver: {
    pending: '还没送出',
    delivered: '已送到 {{agent}}',
    toManager: '已交给管理 bot',
    picked: '{{agent}} 已接手',
    undo: '撤回',
    copy: '复制消息',
    recorded: '已记下',
    notDelivered: '没送出：{{reason}}',
    reason: {
      'no-manager': '这个意图还没有管理者',
      'manager-gone': '管理者没在运行',
      'runtime-down': '连不上 Herdr',
      'runtime-error': 'Herdr 没有接收',
      'no-workspace': '它的意图没有对应的 Herdr 工作区',
      timeout: '没有及时回答',
      restarted: '服务重启了',
      unknown: '没有说明原因'
    },
    failed: '没记下：服务没有收下'
  },
  decide: {
    label: '对 {{id}} 做决定',
    note: '给 agent 的备注（可选）',
    noteShort: '备注',
    accept: '接受',
    revise: '修改',
    reject: '驳回',
    defer: '延后',
    reviseNeedsNote: '选“修改”要在备注里写清怎么改。',
    suggested: '（agent 建议这样）'
  },
  steps: {
    copy: '复制命令',
    copied: '已复制'
  },
  review: {
    found_other: '{{by}} {{count, number}} 个问题',
    passed: '{{by}} 没有问题',
    running: '{{by}} 进行中'
  },
  finding: {
    open: '等你决定',
    rejected: '已驳回',
    deferred: '延后',
    managerDecides: '由管理者决定',
    noManager: '没有在运行的管理者：由你决定',
    notes_other: '{{count, number}} 条补充',
    noted: '{{by}} 补充',
    override: '这是管理者决定的。你可以改成别的决定：',
    scheduled: '已排期',
    done: '已完成',
    dropped: '已放弃',
    revised: '修改',
    acceptedHere: '已接受',
    suggests: '建议{{decision}}',
    decision: { accept: '接受', reject: '驳回', defer: '延后' },
    byAgent: '由 agent 决定',
    leadsTo: '引出的任务',
    causes: '起因',
    details: '{{id}} 的详情',
    waiting_other: '{{count, number}} 个问题等你决定'
  },
  task: {
    blind: '盲审',
    blindHint: '盲审：没有看到其他工作就做的，所以它的发现是独立的看法',
    unregistered: '未登记：{{title}}'
  },
  who: {
    writing: '{{name}}，正在写'
  },
  flow: {
    empty: '这个单元还没有记录任何任务。',
    controls: '图的控制',
    zoomIn: '放大',
    zoomOut: '缩小',
    fit: '显示整张图',
    start: '开始'
  },
  io: {
    input: '输入',
    output: '输出',
    amended: '{{when}} 改过',
    field: '{{name}}：',
    problem: '哪里不对',
    impact: '不处理的后果',
    evidence: '出处',
    changesOf: '{{label}} 的改动',
    noChanges: '还没有改动',
    noFindingsYet: '还没有。',
    noFindings: '没有发现问题。',
    unknownInput: '提交没有写它对应哪个任务，所以不知道它的输入。',
    linkTo: '它属于任务',
    linked: '手动关联',
    rebasedFrom: '变基自 {{hash}}',
    lostCommits: '它的提交不在当前分支上：{{commits}}（在这个单元离开的分支上）',
    uncommitted: '正在写，还没有提交',
    notStarted: '还没有开始。',
    notDone: '还没有做。',
    willChange: '预计会改：{{paths}}'
  },
  stat: {
    files_other: '{{count, number}} 个文件'
  },
  list: {
    sep: '、'
  },
  legend: {
    done: '做完',
    running: '正在做',
    todo: '没开始',
    waiting: '有问题等你决定',
    flowed: '已经流过',
    flowing: '正在流',
    also: '另一个输入'
  },
  block: {
    activity: '活动',
    code: '代码'
  },
  file: {
    back: '全部改动',
    from: '改动来自',
    unchanged: '这个单元还没有改动 {{path}}。'
  },
  tree: {
    label: '文件',
    onlyChanged: '只显示改过的',
    filter: '筛选文件…',
    noMatch: '没有匹配的文件',
    collapse: '收起文件树',
    expand: '展开文件树',
    resize: '文件树的宽度',
    planned: '计划会改，还没动',
    ghost: '计划新增，还不存在'
  },
  change: {
    A: '新增',
    M: '修改',
    D: '删除'
  },
  code: {
    none: '没有改动。',
    moreFiles_other: '还有 {{count, number}} 个文件'
  }
}
