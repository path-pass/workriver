# 把数据库迁移到虚拟工作区（只做一次）

这个版本的 workriver 有自己的工作区（`ws1`：一个 id、一个只用于显示的名字、一个项目），由你创建；manager 由你指定。你现有的数据库是旧格式写的：那时工作区就是 agent 运行时的分组，manager 自己登记。这个版本不读旧格式：`workriver serve` 会拒绝这样的数据库，不改动它，并指向这个文件夹。

这个文件夹把你数据库的一份副本转换一次。它不属于 workriver：是两个独立的 Cargo 包，从不随 workriver 构建或发布；你的数据库迁移完成后，就从仓库里删掉。

- `old/` 用写出这些数据的代码（改动前最后一个提交的 workriver）读副本，写出新事件：先 `old plan`，再 `old apply`。
- `new/` 用这个版本的代码把新事件存进新库，再和旧状态对比：先 `new write`，再 `new check`。

这里没有任何步骤写你的数据库。副本只以只读方式打开；每个输出都是新文件或新文件夹；数据目录（`$WORKRIVER_HOME`，没有时是 `~/workriver-data`）里的任何路径都会被拒绝。

## 开始之前

- 先回答还开着的选择题，并让还没送达的消息送达：迁移之后，开着的选择题不能再回答，没送达的消息也不会再送（它等的那个 manager 已经不在位）。`old plan` 会把它们列出来。
- 下面的命令都在仓库根目录下运行，用 `mise.toml` 指定的 Rust。

## 1. 旧代码和副本

先停掉 `workriver serve`（Ctrl-C），然后：

```sh
git worktree add --detach .local/migrate-old 1a6a6a4   # 改动前的最后一个提交
mkdir -p .local/migrate
cp ~/workriver-data/workriver.db .local/migrate/copy.db
```

这个提交是这次改动最后一次合入的 main，也就是切换之前你的服务在跑的代码，所以它认得你的库里的每一种事件（更早的提交不认得 `agent.keys`，即在页面上回应卡住的 agent 时记下的事件，读到它就停下）。如果你的服务跑的是更新的 main，就换成那个提交。只在服务停着的时候复制。如果有 `workriver.db-wal` 且不是空的，也一起复制，命名为 `copy.db-wal`。原库保持原样：回退就靠它。这期间服务可以再开起来；但复制之后它记录的内容不在迁移里，所以正式切换前要重新复制一次（并重新 plan）。

## 2. Plan

```sh
alias old='cargo run -q --manifest-path scripts/migrate-virtual-workspaces/old/Cargo.toml --'
alias new='cargo run -q --manifest-path scripts/migrate-virtual-workspaces/new/Cargo.toml --'
old plan .local/migrate/copy.db .local/migrate/mapping
```

`plan` 打印并写出映射文件：每条旧的工作区记录变成什么；它判断不了的地方各占一行，用 `?` 标出。把每个 `?` 填上；想改名字或 `open` / `closed` 的，也可以改。以 `#` 开头的行说明每一处的来源：其中来自旧库的文字（标签、名字、文件夹）都写成 JSON 字符串，不会自成一行。每个键只写一次：同一个文件夹、同一条记录或标签的工作区（用项目的 id 或文件夹指都算同一个）、同一条消息或意图写在两行里，会被拒绝，并指出这两行。

| 行 | 你写什么 |
| --- | --- |
| `folder "<path>" = ?` | 某个意图（或 `workspace project`）写过、但没有已登记项目包含的文件夹。`register`：你把它登记为项目（记为你的操作，放在第一次提到它的事件之前）；`into <project>`：它的意图属于那个项目；`none`：它们不属于任何项目（计划中的意图进 “No project” 的路线图）。注释里写着是谁写的这个文件夹：脚本从不自己登记文件夹，因为它可能是某个 agent 写的。 |
| `record <n> <project> = name "...", open` | 每条旧记录、每个项目一个工作区。项目位置是 `?`：没有任何东西说明这条记录属于哪个项目（它的意图都没有项目），写上它所属的项目。意图分属几个项目的记录会被拆开，每个项目一行。 |
| `label "<label>" <project> = name "...", closed` | 发往某个没有任何记录用过的名字的消息：给它们一个关闭的工作区，放在你写的项目里。 |
| `message <id> = ?`、`intent <id> = ?` | 被拆开的记录里的一条消息或消息副本（或一个没有项目的意图）：它去哪个项目的工作区。 |

项目用注释里列出的 id（如 `p2`）或它的文件夹来写。

## 3. Apply、write、check

```sh
old apply .local/migrate/copy.db .local/migrate/mapping .local/migrate/out
new write .local/migrate/out .local/migrate/new.db
new check .local/migrate/out .local/migrate/new.db
```

- 还剩 `?`，或映射文件有没定下来的地方（它会说要加哪一行）时，`apply` 拒绝执行，什么也不写。它为之编写的那个库里没有的两种历史，它也拒绝，并说明怎么办：意图在它的项目登记之前就开始了；以及一个工作区的已读位置保不住的已读标记（运行时改过名的对话，在一个标签下读过、另一个标签下没读：先在旧版本里把它读到底）。它会打印建了哪些工作区、丢弃了什么。`out/` 里有：`events.jsonl`（按顺序的新事件）、`seats.jsonl`（旧的登记，只给 check 用）、`old-state.json`（旧状态，以及映射把每条记录和消息放到哪里）。
- `write` 用这个版本的存储建 `new.db`（所以它是新格式），每个事件保留自己的时间和作者。脚本写出或改过的每个事件，先在它之前的状态上过一遍这个版本的规则，结果必须和写出的一样；原样保留的事件不过规则（有些是在后来收紧的规则之前被接受的）。有任何一条被拒，就列出每一条及规则原话，什么也不写。
- `check` 用这个版本重放 `new.db` 的一份副本，再和旧状态对比：工作区以外的都相等；每个意图、每条消息都在映射说的地方；未读数相同；没有开着的选择题；没有设置任何 manager。映射解释不了的差异会让它失败，并逐条列出。

## 4. 先看，再切换

切换之前，用单独的数据目录和另一个端口在页面上看一遍：

```sh
mkdir -p .local/migrate/home && cp .local/migrate/new.db .local/migrate/home/workriver.db
WORKRIVER_HOME=$PWD/.local/migrate/home workriver serve --port 47830
```

看左侧栏（项目、工作区、意图、单元）、每个对话、Needs you。然后在服务停着的时候：

```sh
mv ~/workriver-data/workriver.db ~/workriver-data/workriver.db.before-virtual-workspaces
cp .local/migrate/new.db ~/workriver-data/workriver.db
```

（如果有旧的 `-wal` 和 `-shm` 文件，也一起移开。）然后启动新版本。

## 之后

- **给每个工作区指定 manager。** 开始时所有位置都是空的：agent 自己登记不等于你的操作。Needs you 会列出每个有进行中意图、却没有在跑的 manager 的工作区；在那里选它的 manager（或者在工作区那一行点 Manager；或者在你自己的终端里运行 `workriver workspace manager <ws> <agent>`）。plan 的注释里列着最后登记的是谁。
- 链接没有了：另一台机器上的 agent 通过分给它的任务，在同一个工作区里工作。
- **回退**：用旧库加旧版本的程序。
- 之后删掉这个文件夹和 `.local/migrate-old`（`git worktree remove .local/migrate-old`）。

## 逐行说明它做了什么

| 旧 | 新 |
| --- | --- |
| 一条工作区记录 | `workspace.created`（你的操作），放在第一个需要它的事件之前，且在它的项目登记之后。名字：它最后的标签。到最后如果里面没有进行中的意图，就关闭。 |
| 意图分属几个项目的记录 | 每个项目一个工作区。 |
| 按标签合并的记录（重启后换了 id） | 一个工作区。 |
| 同一个标签、另一台机器、没有链接 | 各自一个工作区。 |
| 链接（`workspace.linked` / `unlinked`） | 丢弃。链接有效期间，被链接的分组记录的内容归到它被链接到的那个工作区；之前和之后，归到它自己那条记录的工作区。 |
| `manager.registered` | 丢弃；所有位置开始时都是空的。 |
| `workspace.project_set` | 并入工作区的项目（在它的意图都没有项目时使用）；丢弃。 |
| 一次启动、在某个分组里建的意图、`intent.linked` | 带工作区 id 的启动；`intent.workspace_set`（你的操作）。 |
| `intent.created` 里的运行时分组 | 从事件里去掉；其余照原样。 |
| `message.sent`、`message.said`、发给所有 manager 的每份副本 | 记录当时消息去的那个工作区的 id。 |
| 按标签的 `messages.read` | 当时用过这个标签的每个工作区各一条。 |
| 映射里 `register` 的文件夹 | `project.added`（你的操作），放在第一次提到它的事件之前；之后登记的项目 id 顺延一位，它的改名也跟着改。 |
| 没有工作区的已登记项目 | 在最后建一个 `workspace.created`，名字取项目名。 |
| 其他所有事件 | 照原样，一个字节都不变。 |

## 测试

```sh
cargo test --manifest-path scripts/migrate-virtual-workspaces/old/Cargo.toml
cargo test --manifest-path scripts/migrate-virtual-workspaces/new/Cargo.toml
```

`old` 的测试在旧事件的样例上跑上面每一行，并把 `apply` 写出的结果保存在 `new/tests/__fixtures__/`（`WORKRIVER_BLESS=1` 时重新生成）；`new` 的测试对它们做 write 和 check，并验证：规则拒绝的写出事件、或结果与写出不一致的事件，会让运行失败；原样保留的旧事件不过今天的规则；无法解释的差异会让 check 失败。
