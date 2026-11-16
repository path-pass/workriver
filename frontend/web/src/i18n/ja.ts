// Japanese UI text. Typed against en.ts so a missing or extra key is a compile error.
// Japanese has one plural form, so the `_one` keys are left out.
import type { en, OneForm } from './en'

export const ja: OneForm<typeof en> = {
  rail: {
    note: 'メモ',
    noteMore: 'もっと見る',
    noteLess: '折りたたむ',
    shortcut: '{{label}}（⌘B / Ctrl+B）',
    fold: 'サイドバーを閉じる',
    unfold: 'サイドバーを開く',
    resize: 'サイドバーの幅（ドラッグ、または矢印キー。ダブルクリックで既定の幅）',
    findings: '指摘を判断する人',
    findingsBy: { user: '自分だけ', manager: 'マネージャー' },
    search: 'ユニットを検索',
    noMatch: '「{{q}}」に一致するユニットはありません',
    clearSearch: '検索をクリア',
    none: 'まだユニットが記録されていません。',
    noUnits: 'ユニットはまだありません',
    project: 'プロジェクト',
    allProjects: 'すべてのプロジェクト',
    active: '進行中',
    roadmap: 'ロードマップ',
    done: '完了',
    add: '追加',
    addTitle: 'タイトル',
    addGoal: '目標（一文で）',
    addSave: '追加',
    addCancel: 'キャンセル',
    addPick: 'ロードマップに追加するには、上でプロジェクトを選んでください',
    drag: 'ドラッグで並べ替え',
    start: '開始',
    intentWork: '調査と設計',
    failed: '保存できませんでした：{{error}}',
    mergeOpen: '統合…',
    mergeTitle: '計画中のインテントを統合',
    mergeHint: '同じプロジェクトのインテントを 2 つ以上選びます。1 つを残し、ほかは取り下げられて、その内容は残すインテントのメモに入ります。',
    mergeInto: '残すインテント',
    mergeNewTitle: 'そのタイトル',
    mergeWhat: '{{from}}は取り下げられ、そのタイトル・目標・メモ・リンクは{{into}}のメモに入ります。',
    mergeRename: '「{{into}}」の名前が変わります。',
    quoted: '「{{title}}」',
    merge: '統合',
    held: {
      tasks: '独自のタスクがあるため統合できません（理由を添えて取り下げることはできます）。',
      units: '独自のユニットがあるため統合できません（理由を添えて取り下げることはできます）。',
      findings: 'バックログに独自の指摘があるため統合できません（理由を添えて取り下げることはできます）。',
      chores: 'あなたへの独自の依頼があるため統合できません（理由を添えて取り下げることはできます）。'
    }
  },
  manager: {
    mark: 'このワークスペースのマネージャー',
    of: 'マネージャー {{name}}：{{state}}',
    state: { working: '作業中', idle: '待機中', done: '待機中', blocked: '停止中', unknown: '状態不明', gone: '実行されていません', none: 'マネージャーなし' }
  },
  start: {
    title: 'はじめに',
    lead: 'まだ何も記録されていません。エージェントは自分の作業を自分で記録し、Herdr ワークスペースごとに 1 つのマネージャーエージェントが計画します。',
    agent: 'Herdr ワークスペースでコーディングエージェントを起動し、そのマネージャーにします。',
    skill: 'マネージャースキルを渡します：skills/workriver-manager をそのスキルフォルダにリンクするか、このコマンドの出力を貼り付けます。',
    register: 'ワークスペースのマネージャーとして登録させます：',
    begin: '次に、左のロードマップから目標を開始させます（先に `workriver backlog add` で追加することもできます）：',
    import: '既存のバックログファイルがあれば、エージェントにインポートスキルで取り込ませます：',
    work: 'マネージャーが目標をユニットとタスクに分けてワーカーエージェントに渡し、その作業がここに表示されます。',
    trailer: 'コミットメッセージに次の行を書くと、そのコミットはタスクに属します：'
  },
  session: {
    title: 'workriver',
    lead: 'このページはあなたとして判断し操作するため、あなた自身のターミナルから得たリンクでしか開けません。',
    open: 'workriver が動いているマシンで、エージェントのペインではないターミナルから：',
    why: 'リンクは 1 回だけ、5 分間有効です。その後このブラウザはログインしたままになります。エージェントや他のマシンがあなたとして操作することはできません。'
  },
  service: {
    unreachable: 'workriver サービスに接続できません。`workriver serve` は実行中ですか？'
  },
  settings: {
    title: '設定',
    close: '閉じる',
    language: '言語',
    theme: 'テーマ',
    themes: { system: 'システム', light: 'ライト', dark: 'ダーク' },
    notify: 'エージェントが私を待っているときに通知する',
    notifyDenied: 'このページの通知はブラウザで許可されていません。'
  },
  machines: {
    title: '他のマシン',
    lead: '他のマシンのエージェントを ssh 経由で追跡します（各マシンでそれぞれの Herdr が動きます）。',
    none: 'まだありません。名前と `ssh` に渡す接続先で追加します（パスワードを聞かれずに接続できる必要があります）。',
    on: '{{machine}} 上',
    unreachable: '{{machine}} に接続できません。最後に確認した状態を表示しています',
    follow: '{{machine}} を追跡',
    connected_other: '接続済み、エージェント {{count, number}} 件',
    noHerdr: '接続済み、ただし Herdr が応答しません',
    down: '未接続',
    connecting: '接続中…',
    off: '追跡していません',
    downSince: '{{time}} から切断中',
    retry_other: '{{count, number}} 秒後に再接続',
    disconnected: '{{machine}} は {{time}} から切断されています。最後に確認した状態を表示しています',
    name: '名前',
    ssh: 'ssh の接続先（ホストまたは user@host）',
    add: '追加',
    remove: '{{machine}} を削除',
    links: '向こうのワークスペース',
    linkLead: '向こうのワークスペースは、こちらのワークスペースとリンクしたときだけ同じものになります。マネージャーは 1 人、インテントも共通です。',
    notLinked: 'リンクなし',
    linkTo: '{{machine}} の {{workspace}} をこちらのワークスペースにリンク',
    failed: '完了できませんでした：{{error}}'
  },
  unit: {
    progress: {
      working: '作業中',
      waiting: '待ち',
      ready: 'マージ可能',
      merged: 'マージ済み',
      done: '終了'
    },
    complete: 'すべてのユニットが終わりました。マネージャーがこのインテントを閉じられます',
    finished: '完了',
    previously: '以前のブランチ：{{branches}}',
    blocking_other: 'エージェントが待っている手動ステップ {{count, number}} 件',
    active: '進行中',
    untouched: '未着手',
    waiting_other: 'あなた待ち {{count, number}} 件',
    grew_other: '計画外 {{count, number}}',
    grewHint: 'ブランチが当初の計画を超えて広がっています'
  },
  agents: {
    blocked: 'ターミナルであなたを待っています',
    notice: '{{agent}} があなたを待っています',
    title: 'エージェント',
    idle: '待機中'
  },
  plan: {
    title: '「{{title}}」を開始',
    lead: 'workriver はエージェントを起動しません。Herdr で次の手順を行います：',
    workspace: 'このための Herdr ワークスペースを開くか、マネージャーにまだインテントを任せられるワークスペースを選びます。',
    agent: 'ワークスペースにまだマネージャーがいなければ、そこでコーディングエージェントを起動してマネージャースキルを渡します：',
    register: 'マネージャーのペインで登録します：',
    start: 'そこでこのインテントを開始します。そのワークスペースの新しいユニットとタスクはこのインテントに入ります：'
  },
  backlog: {
    title: 'バックログ',
    lead: '保留した指摘。実施を待っています。',
    none: 'ここに保留した指摘はありません。',
    every: 'すべてのプロジェクトです。サイドバーで 1 つ選ぶと絞り込めます。',
    together: 'まとめて実施',
    drop: '取り下げ',
    dropReason: '取り下げる理由は？',
    dropConfirm_other: '{{count, number}} 件を取り下げ',
    cancel: 'キャンセル',
    selected_other: '{{count, number}} 件を選択中',
    for: '実施先',
    startFirst: '先に {{intent}} を開始してください。マネージャーが `workriver intent start {{intent}}` を実行します。',
    oneIntent: '1 つのインテントの指摘だけを選んでください。',
    noIntent: 'このプロジェクトには、実施先になる進行中のインテントがありません。',
    sent: '1 通のメッセージとして {{to}} に送りました。',
    scheduled: '予定済み',
    fromSource: '出典：{{source}}',
    added: 'バックログに直接追加',
    missing: '影響や場所の記載なし',
    unit: '{{intent}} · {{unit}}'
  },
  side: {
    title: '要対応・エージェント・コミット・バックログ・理由を聞く',
    close: 'パネルを閉じる'
  },
  terminal: {
    open: 'ターミナルで開く',
    openFor: '{{agent}} をターミナルで開く',
    note: '{{agent}} のターミナルを前面に表示します（未接続）'
  },
  evidence: {
    label: '実行したチェック',
    passed: '成功',
    failed: '失敗',
    none: 'テスト未実行'
  },
  ask: {
    placeholderGate: '{{agent}}にこのステップを質問…',
    placeholderManager: 'マネージャーボットに質問…',
    why: '理由を聞く',
    label: 'この作業についての質問',
    placeholder: '{{who}}に理由を質問…',
    send: '送信',
    remove: '{{label}} を外す',
    how: '一時的なエージェントが記録とコードから回答します。{{agent}} が待機中のときだけ {{agent}} に確認します。',
    howAlone: '一時的なエージェントが記録とコードから回答します。',
    step: {
      starting: 'エージェントを起動中…',
      reading: '記録とコードを読んでいます…',
      asking: '{{agent}} に確認中…'
    },
    asked: '{{agent}} に確認済み',
    busy: '{{agent}} が作業中のため、記録から回答しました',
    ended: '{{agent}} のセッションは終了しているため、記録から回答しました',
    cannot: '回答するエージェントを起動できませんでした：{{reason}}',
    end: '終了',
    endHint: 'この会話を終えます。担当のエージェントも終了します',
    threadEnded: '終了',
    conclude: '結論を起草',
    concludeAsked: '結論を依頼しました',
    conclusion: '結論',
    fresh: '新しい質問',
    exchanges_other: 'やり取り {{n}} 回',
    earlier_other: '以前の会話 {{count, number}} 件',
    state: {
      answered: '回答済み',
      waiting: '回答待ち',
      failed: '失敗',
      ended: '終了'
    },
    empty: 'グラフでタスクを、またはコードで行を選ぶと、その理由を聞けます。',
    conversations_other: '会話 {{count, number}} 件',
    count_other: '質問 {{count, number}} 件'
  },
  chore: {
    done: '完了'
  },
  commits: {
    title: 'コミット',
    writing: '作成中',
    today: '今日',
    yesterday: '昨日'
  },
  inbox: {
    title: '要対応',
    blocking: 'ブロック中',
    blockedAgent: '{{agent}} がターミナルであなたを待っています',
    decisions: '判断待ち',
    gaps: '抜け',
    later: '急ぎではない',
    empty: '対応が必要なものはありません。',
    suggested_other: 'エージェントの提案あり {{count, number}} 件'
  },
  need: {
    loose: '未登録',
    far: '計画外に拡大'
  },
  kind: {
    implement: '実装',
    design: '設計',
    research: '調査',
    gate: '手動ステップ',
    review: 'レビュー'
  },
  state: {
    todo: '次に実行',
    running: '進行中',
    done: '完了',
    waiting: 'あなた待ち',
    unregistered: '未登録'
  },
  reviewer: {
    'security-review': 'セキュリティレビュー',
    'code-review': 'コードレビュー',
    test: 'テスト'
  },
  gate: {
    deploy: '手動デプロイ',
    test: '手動テスト',
    decide: '設計の確認',
    result: '結果',
    done: '完了',
    problem: '問題あり',
    note: 'メモ（スクリーンショットはここに貼り付け）',
    finding: '指摘 {{n}}',
    severity: '重大度',
    where: '場所：ファイルパスをカンマか改行で区切って入力',
    addFinding: '指摘を追加',
    removeFinding: 'この指摘を削除',
    needFinding: '「問題あり」には、何が問題かを書いた指摘が少なくとも 1 件必要です。',
    submit: '送信'
  },
  attach: {
    add: '添付',
    hint: 'または画像を貼り付け／ドロップ',
    open: '{{name}} を拡大表示',
    remove: '{{name}} を削除',
    zoomIn: '幅いっぱいに表示（画像のクリックでも切り替え）',
    zoomOut: '画像全体を表示（画像のクリックでも切り替え）',
    original: '元の画像を新しいタブで開く'
  },
  deliver: {
    pending: '未送信',
    delivered: '{{agent}} に送信しました',
    toManager: 'マネージャーボットに送信しました',
    picked: '{{agent}} が受け取りました',
    undo: '取り消す',
    copy: 'メッセージをコピー',
    recorded: '記録しました',
    notDelivered: '送信できませんでした：{{reason}}',
    reason: {
      'no-manager': 'このインテントにマネージャーがいません',
      'manager-gone': 'マネージャーが実行されていません',
      'runtime-down': 'Herdr に接続できません',
      'runtime-error': 'Herdr に拒否されました',
      'no-workspace': 'インテントに Herdr ワークスペースがありません',
      timeout: '時間内に応答がありません',
      restarted: 'サービスが再起動しました',
      unknown: '理由が不明です'
    },
    failed: '記録できませんでした：サービスが受け付けませんでした'
  },
  decide: {
    label: '{{id}} を判断',
    note: 'エージェントへのメモ（任意）',
    noteShort: 'メモ',
    accept: '採用',
    revise: '修正',
    reject: '却下',
    defer: '保留',
    reviseNeedsNote: '「修正」には、どう直すかを書いたメモが必要です。',
    suggested: '（エージェントの提案）'
  },
  steps: {
    copy: 'コマンドをコピー',
    copied: 'コピーしました'
  },
  review: {
    found_other: '{{by}}：指摘 {{count, number}} 件',
    passed: '{{by}}：問題なし',
    running: '{{by}}：進行中'
  },
  finding: {
    open: 'あなた待ち',
    rejected: '却下',
    deferred: '保留',
    managerDecides: 'マネージャーが判断します',
    noManager: 'マネージャーが実行されていません：あなたが判断してください',
    notes_other: '追記 {{count, number}} 件',
    noted: '{{by}} が追記',
    override: 'マネージャーが判断しました。別の判断もできます：',
    scheduled: '予定済み',
    done: '完了',
    dropped: '取り下げ',
    revised: '修正',
    acceptedHere: '採用',
    suggests: '提案：{{decision}}',
    decision: { accept: '採用', reject: '却下', defer: '保留' },
    byAgent: 'エージェントが判断',
    leadsTo: '後続',
    causes: '指摘',
    details: '{{id}} の詳細',
    waiting_other: '判断待ちの指摘 {{count, number}} 件'
  },
  task: {
    blind: 'ブラインド',
    blindHint: 'ブラインド：他の作業を見ずに行ったため、指摘はこのレビュー独自の見方です',
    unregistered: '未登録：{{title}}'
  },
  who: {
    writing: '{{name}}、作成中'
  },
  flow: {
    empty: 'このユニットにはまだタスクが記録されていません。',
    start: '開始',
    controls: 'グラフの操作',
    zoomIn: '拡大',
    zoomOut: '縮小',
    fit: 'グラフ全体を表示'
  },
  io: {
    input: '入力',
    output: '出力',
    amended: '{{when}} に修正',
    field: '{{name}}：',
    problem: '問題点',
    impact: '放置した場合',
    evidence: '場所',
    changesOf: '{{label}} による変更',
    noChanges: 'まだ変更はありません',
    noFindingsYet: 'まだありません。',
    noFindings: '問題は見つかりませんでした。',
    unknownInput: 'コミットにタスクの記載がないため、入力は不明です。',
    linkTo: '属するタスク',
    linked: '手動でリンク',
    rebasedFrom: '{{hash}} からリベース',
    lostCommits: 'コミットが現在のブランチにありません：{{commits}}（このユニットが離れたブランチ上）',
    uncommitted: '作成中、未コミット',
    notStarted: 'まだ開始していません。',
    notDone: 'まだ完了していません。',
    willChange: '変更予定：{{paths}}'
  },
  stat: {
    files_other: '{{count, number}} ファイル'
  },
  list: {
    sep: '、'
  },
  legend: {
    done: '完了',
    running: '進行中',
    todo: '未着手',
    waiting: 'あなた待ちの指摘',
    flowed: '受け渡し済み',
    flowing: '受け渡し中',
    also: '副次的な入力'
  },
  block: {
    activity: 'アクティビティ',
    code: 'コード'
  },
  file: {
    back: 'すべての変更',
    from: '変更元',
    unchanged: 'このユニットはまだ {{path}} を変更していません。'
  },
  tree: {
    label: 'ファイル',
    onlyChanged: '変更のみ表示',
    filter: 'ファイルを絞り込み…',
    noMatch: '一致するファイルはありません',
    collapse: 'ファイルツリーを閉じる',
    expand: 'ファイルツリーを開く',
    resize: 'ファイルツリーの幅',
    planned: '変更予定、未変更',
    ghost: '予定、まだ存在しません'
  },
  change: {
    A: '追加',
    M: '変更',
    D: '削除'
  },
  code: {
    none: '変更はありません。',
    moreFiles_other: 'ほか {{count, number}} ファイル'
  }
}
