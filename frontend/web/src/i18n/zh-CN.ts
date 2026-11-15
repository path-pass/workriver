// Chinese UI text. Typed against en.ts so a missing or extra key is a compile error.
// Chinese has one plural form, so the `_one` keys are left out.
import type { en, OneForm } from './en'

export const zhCN: OneForm<typeof en> = {
  rail: {
    shortcut: '{{label}}（⌘B / Ctrl+B）',
    fold: '收起侧栏',
    unfold: '展开侧栏',
    search: '搜索单元',
    noMatch: '没有单元匹配「{{q}}」',
    clearSearch: '清除搜索',
    none: '还没有记录任何单元。'
  },
  manager: {
    mark: '这个意图的管理者',
    of: '管理者 {{name}}：{{state}}',
    state: { working: '工作中', idle: '空闲', done: '空闲', blocked: '卡住了', unknown: '状态不明', gone: '没在运行', none: '没有管理者' }
  },
  service: {
    unreachable: '连不上 workriver 服务。`workriver serve` 在运行吗？'
  },
  settings: {
    title: '设置',
    close: '关闭',
    language: '语言',
    theme: '主题',
    themes: { system: '跟随系统', light: '浅色', dark: '深色' }
  },
  unit: {
    blocking_other: '{{count, number}} 个手动步骤卡住了 agent',
    active: '进行中',
    untouched: '没动手',
    waiting_other: '{{count, number}} 件事等你',
    grew_other: '长出原计划 {{count, number}}',
    grewHint: '有分支长到了原计划之外'
  },
  agents: {
    title: 'Agent',
    idle: '空闲'
  },
  side: {
    title: '需要你、Agent 和提交',
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
      unknown: '没有说明原因'
    },
    failed: '没记下：服务没有收下'
  },
  decide: {
    label: '对 {{id}} 做决定',
    note: '给 agent 的备注（可选）',
    noteShort: '备注',
    accept: '接受',
    reject: '驳回',
    defer: '延后',
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
