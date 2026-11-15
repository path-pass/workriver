// Japanese UI text. Typed against en.ts so a missing or extra key is a compile error.
// Japanese has one plural form, so the `_one` keys are left out.
import type { en, OneForm } from './en'

export const ja: OneForm<typeof en> = {
  rail: {
    shortcut: '{{label}}（⌘B / Ctrl+B）',
    fold: 'サイドバーを閉じる',
    unfold: 'サイドバーを開く',
    search: 'ユニットを検索',
    noMatch: '「{{q}}」に一致するユニットはありません',
    clearSearch: '検索をクリア',
    none: 'まだユニットが記録されていません。'
  },
  manager: {
    mark: 'このインテントのマネージャー',
    of: 'マネージャー {{name}}：{{state}}',
    state: { working: '作業中', idle: '待機中', done: '待機中', blocked: '停止中', unknown: '状態不明', gone: '実行されていません', none: 'マネージャーなし' }
  },
  service: {
    unreachable: 'workriver サービスに接続できません。`workriver serve` は実行中ですか？'
  },
  settings: {
    title: '設定',
    close: '閉じる',
    language: '言語',
    theme: 'テーマ',
    themes: { system: 'システム', light: 'ライト', dark: 'ダーク' }
  },
  unit: {
    blocking_other: 'エージェントが待っている手動ステップ {{count, number}} 件',
    active: '進行中',
    untouched: '未着手',
    waiting_other: 'あなた待ち {{count, number}} 件',
    grew_other: '計画外 {{count, number}}',
    grewHint: 'ブランチが当初の計画を超えて広がっています'
  },
  agents: {
    title: 'エージェント',
    idle: '待機中'
  },
  side: {
    title: '要対応・エージェント・コミット',
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
    remove: '{{name}} を削除'
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
      unknown: '理由が不明です'
    },
    failed: '記録できませんでした：サービスが受け付けませんでした'
  },
  decide: {
    label: '{{id}} を判断',
    note: 'エージェントへのメモ（任意）',
    noteShort: 'メモ',
    accept: '採用',
    reject: '却下',
    defer: '保留',
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
