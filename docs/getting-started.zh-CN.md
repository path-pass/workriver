# 上手

[English](getting-started.md)

在一台装好 [mise](https://mise.jdx.dev)、git 和 agent 运行时（[Herdr](https://herdr.dev)；它需要什么：
[runtimes/herdr.zh-CN.md](runtimes/herdr.zh-CN.md)）的 Mac 上，把 workriver 跑起来，并给每个工作区
指定一个管理者 agent。

## 1. 构建

```sh
git clone https://github.com/path-pass/workriver.git
cd workriver
mise install                 # 按 mise.toml 装好 Rust、Node 和 pnpm，只对这个目录生效
cd frontend/web && pnpm install && pnpm build && cd ../..
cargo build --release
```

如果 shell 里没有启用 mise，在命令前加 `mise exec --`。网页界面会打进二进制文件里，所以要先构建界面。

二进制文件在 `target/release/workriver`。我们不替你安装，下面任选一种：

- 在 shell 的启动文件里把这个目录加进 PATH：
  `export PATH="$HOME/<路径>/workriver/target/release:$PATH"`；
- 或者把它链接到已经在 PATH 里的目录，例如
  `ln -s "$PWD/target/release/workriver" ~/.local/bin/workriver`；
- 或者 `cargo install --path crates/workriver`（复制到 `~/.cargo/bin`，每次更新后要再跑一次）。

agent 会在自己的 shell 里调用 `workriver`，所以它们看到的 PATH 里也要有它。

## 2. 运行服务

在运行时里一个单独的终端中启动它，这样它能找到运行时，也会一直运行（Herdr 在哪里找：
[runtimes/herdr.zh-CN.md](runtimes/herdr.zh-CN.md#安装和运行)）：

```sh
workriver serve              # http://127.0.0.1:47820
```

然后在一个不属于任何 agent 的终端里运行 `workriver open`：它用一个一次性的链接在浏览器里打开页面，
让这个浏览器以你的身份登录。这个链接不会被打印出来，因为 agent 读得到运行时里的终端；在没有浏览器的
机器上，`workriver open --print` 会打印一个只有一分钟有效的链接：请在 agent 读不到的终端里用。直接打开地址只会看到"怎么打开"
的说明；原因见[安全模型](security.zh-CN.md)。

打开 <http://127.0.0.1:47820>。`workriver status` 应该显示 `agent runtime: connected`。

- 数据：`~/workriver-data/workriver.db`（设 `WORKRIVER_HOME` 换位置）。只有一个 SQLite
  文件；备份时先停掉服务再复制它。附件（截图、报告）放在旁边的 `attachments/` 里，同样的
  内容只存一份，和记录一样永不删除；`workriver status` 会显示它们占了多少空间。
- 页面上的终端按钮会在运行时里把 agent 的终端切到前面，再把终端应用切到前台（默认
  Ghostty；用 `workriver serve --terminal-app iTerm` 改，传 `''` 表示不切）。

## 3. 项目和待办

**项目**是一个装着一个或多个仓库的目录。装了好几个仓库的目录要登记一下：在页面上点"添加项目"（左栏
项目列表的底部），填目录的完整路径（从 `/` 或 `~/` 开始），或者运行
`workriver project add ~/code/shop`（它的仓库就是下一层带 `.git` 的子目录）；单个仓库也这样登记。项目必须
是目录而不是文件，也不能在另一个项目里面或者包含另一个项目。
只有你能登记项目，一个仓库要么属于某个已登记的项目，要么不属于任何项目：agent 添加意图或延后的
发现时，要写你登记过的项目（`p1`，或它里面的文件夹），否则会被拒绝，并给出需要你运行的命令。

添加项目时，它会得到一个与项目同名的**工作区**：项目的意图在这里运行，有一个管理者，和你有一份对话。
工作区是 workriver 自己的（`ws1`，显示在名字旁边），不是运行时的：终端在运行时里的位置什么都不决定。
通常一个项目一个工作区；大的意图或者想单独开一份对话时可以再加（左栏项目上的"+"，或者
`workriver workspace add p1 --name "Checkout"`），用完的可以关闭（它的意图和对话都保留）。

每个项目有一份**待办**：它的**路线图**（计划中的意图，按你想要的顺序排的目标）和**延后的发现**
（先放一放的问题）。在页面左栏往路线图里加（Roadmap 旁的"+"，拖动调整顺序），或者：

```sh
workriver backlog add "Offline sync" --goal "Notes can be taken offline and merge later." --project ~/code/shop
workriver backlog list
```

**导入已有的清单。** 每份笔记文件的写法都不一样，所以没有固定的解析器；由一个 agent 用导入
skill 来做（`skills/workriver-backlog-import/`，或者把 `workriver guide import` 的输出贴给它）。
告诉它哪个文件、导进哪个项目；它读文件，分清哪些是目标、哪些是遗留的发现（同一个目标的各个步骤
合成一个意图，作为清单放进它的备注），先给你看一张预览表，
等你同意，再按文件里的顺序用 `workriver backlog add` 和 `workriver backlog add-finding` 加进去。
它不会改动原文件。`workriver backlog export --project P` 把待办再打印成 Markdown。
拆得太碎？点路线图标题旁的"合并…"打开对话框：勾选几个意图，选出保留的那个，需要的话给它一个新标题。
或者 `workriver backlog merge i3 i4 i5 --title "搜索"`：i4 和 i5 被放弃，它们写的内容进到 i3 的
备注里，显示在它的目标下面。agent 加的意图（比如导入的 agent 收拾自己导入的）agent 可以合并；
你自己加的意图只有你能合并。

## 4. 给每个工作区一个管理者

一个工作区跑它所属项目的一个或多个意图，由它的管理者 agent 规划和派活。在运行时里启动一个
coding agent，把 skill 给它，然后指定它为管理者（见下）。

装**存根**，只装一次：存根是很小的 skill 文件，只让 agent 去运行 `workriver guide …`，而后者永远和
已安装的 `workriver` 一致。把真正的 skill 文件拷过去，每次更新后就过期了；照着旧拷贝做事的管理者
不知道已经有了哪些命令。

```sh
for s in manager howto import; do
  mkdir -p ~/.claude/skills/workriver-$s          # Codex: ~/.agents/skills/workriver-$s
  workriver guide install-stub $s > ~/.claude/skills/workriver-$s/SKILL.md
done
```

（只给一个项目：`<项目>/.claude/skills/` 或 `<项目>/.agents/skills/`。有这个仓库的机器上也可以用
链接，`git pull` 之后自动是新的：`ln -s "$PWD/skills/workriver-manager" ~/.claude/skills/workriver-manager`，
`workriver-howto` 和 `workriver-backlog-import` 同理。）其他工具：让 agent 先运行 `workriver guide manager`。

skill 有三个：管理者的角色（`workriver guide manager`，很短）、各种情况的做法（`workriver guide how`
是目录，`workriver guide how <主题>` 是一条做法）、导入待办（`workriver guide import`）。更新之后，
已经在运行的管理者会收到一次提示，让它读 `workriver guide changes`。

**从页面启动一个**：左栏该工作区那一行点"启动管理者"（或者点"管理者"，再选"启动一个新的"）。选 harness，
Claude Code 或 Codex（命令在这台机器上、运行时对它的集成是最新的才可选，否则显示为灰色并说明原因），再选
模型：你的默认、harness 自己认识的别名（Claude Code 是 `opus`、`sonnet`、`fable`），或模型的全名。workriver
在项目目录里开一个终端（放在运行时里与工作区同名的分组），只带这个模型参数启动 harness，等它就绪、有了
会话就指定它为管理者，它会收到和下面一样的通知。它用的是你自己对这个 harness 的配置，和你在那个目录里手动输入
它的名字一样。在 harness 没见过的目录里，它会先问你是否信任这个目录：工作区那一行会显示它在等你，你在它
的终端里或者从"需要你"里回答。harness 启动时报告了问题（比如它不认识的模型）的，不会被指定：那一行显示
harness 自己的原话，终端保留。十分钟内没有会话的，那一行会说明原因，终端保留，等它运行起来后手动指定。
设置 → 启动管理者 里可以设默认的 harness 和模型。

或者**自己指定**：在页面左栏该工作区那一行点"管理者"，从正在运行的 agent 里选（页面会显示每个
agent 的 harness、机器和文件夹）；或者在你自己的终端里

```sh
workriver workspace list                 # ws1  shop  project shop (p1)  no manager
workriver workspace manager ws1 boss     # agent 的名字，或它的会话
```

workriver 会在那个 agent 的终端里告诉它：它现在是这个工作区的管理者，以及它的职责写在哪里。只有你能
指定管理者：agent 不能自己当，席位也不跟着名字或运行时里的位置走。
随时可以换人（被换下的会收到通知），或者清空席位（`--none`）。

管理者的终端关掉后，它的工作区显示"管理者未运行"。同一个会话在同一台机器、同一个文件夹里恢复，会自己
重新成为管理者（对话里会写明）。新的会话，或者原会话换了文件夹，则不会："需要你"会列出这个工作区并建议
那个 agent，点一下就指定了。

计划中的意图旁边的"开始"会列出它所属项目的开着的工作区和各自的管理者；意图只在它所属项目的工作区里运行。

可选：如果 agent 用 Playwright MCP 截图，它的文件默认会落在项目里（`.playwright-mcp/`）。
可以把它自己命名的文件指到 workriver 的目录，例如 Claude Code：
`claude mcp add playwright -- npx @playwright/mcp@latest --output-dir ~/workriver-data/attachments/inbox/playwright`
（或者设 `PLAYWRIGHT_MCP_OUTPUT_DIR`）。这只管没有指定文件名的截图；agent 自己给了文件名的，
仍然相对项目目录保存。之后 agent 用 `workriver attach <文件> --task u1#3 --move` 把需要的附上。

## 5. 接下来会发生什么

开始第一个意图：在路线图里 i1 "Mentions" 上点"开始"，选工作区；它的管理者会被告知去拆分它。之后管理者
在自己所在的工作区里自己开始这个项目的其他意图（`workriver intent start i2`）；别的项目的它开始不了。

管理者在自己的终端里：

```sh
workriver backlog list                 # 路线图：i1 "Mentions"（进行中，由你开始）……
workriver unit add --title "Mentions" --branch feat/mention   # 单元自己的分支：feat/、fix/ 或 chore/ 开头
workriver task add u1 --title "Notify the people mentioned, with unit tests" --agent finder
```

然后它在运行时里启动干活的 agent `finder`，把任务交给它：任务 id、一句话目标、在哪里干、
"运行 workriver task show u1#1"，以及 `workriver guide worker` 的全文（Herdr 下的命令：
[runtimes/herdr.zh-CN.md](runtimes/herdr.zh-CN.md#工作区和管理者)）。

worker 自己记录工作：

```sh
workriver task show u1#1                          # 任务全文：要求、起因、补充说明
workriver task start u1#1 --model claude-opus-5-5  # 它用的模型（知道的话）
git commit -m "Find mentions" -m "Task: u1#1"     # 这一行 trailer 把提交和任务连起来
workriver task deliver u1#1 --evidence 'npm test => passed: 6 passed'
```

管理者尽量少拆：一个意图默认就是一个单元，除非有工作必须分开落地（另一个仓库、另一台机器、
必须单独进行的一次合并）；一个任务是交给一个 agent 的一个完整结果，而不是每一步一个任务。
单元分支上没有 `Task:` 这一行的提交会显示为未登记，是一个要补上的缺口。
每个单元都标出自己的状态：进行中、等待中（有未决定的发现或 gate）、可以合并、已合并（快进或
合并提交 workriver 自己能看到；squash 合并之后要运行 `workriver unit merged u1`）、已结束
（`workriver unit done u1 --reason "……"`，用于不经合并就结束的工作）。一个意图的单元都已合并或
结束时，由管理者用 `workriver intent done i1` 结束这个意图。不再需要的任务（比如分支变了之后重新问的
合并 gate）会被标记为不再需要，而不是删除：在页面上它的详情里点"不再需要…"，或者
`workriver task drop u1#15 --reason "superseded by u1#17"`。它变成灰色、离开需要你、不再挡住任何
后续工作，做它的 agent 和管理者都会收到通知。任务派出去之后才想到的要求，补充到任务上，而不是
直接打进 agent 的终端：`workriver task note u1#3 --text "Also cover the Codex adapter"`，由意图的
管理者或你在任务交付之前补充。补充的要求显示在任务原来的要求下面，带着是谁、什么时候补充的，
页面上和 `workriver task show` 里都能看到；任务已经开始的话，做它的 agent 会收到通知。审查和测试是单独的
任务。它们找出的问题默认由这个意图的管理者决定（带机器人标记；你可以在页面上改掉任何一个决定，
也可以按意图把决定权留给自己：左栏的"发现：自动 / 手动"（选手动），或者 `workriver intent set i1 --findings user`）。
归你决定的问题在页面的 **需要你** 里等着：在那里对每一个接受、按我的方式改、驳回或延后。延后的会留在**待办**面板里；勾选几条，
"一起做"会把它们作为一条消息发给这个意图的管理者，由它合成一个清理单元。你的决定和
手工步骤的结果会通过运行时送到该送的 agent（接受的问题送给管理者，由它派活）；下面那一行
显示有没有送到、有没有被接手，"复制消息"能拿到同样的文字，用来手动粘贴。

## 6. 其他机器

一个 workriver 也能跟踪其他机器上的 agent。每台机器跑自己的运行时；workriver 和每台机器之间保持
一条 ssh 连接，把那台机器的运行时带到这边，同时给那边的 `workriver` 命令行一条回到这个
服务的路。

要求：那台机器上的运行时正在运行（Herdr 要什么版本：[runtimes/herdr.zh-CN.md](runtimes/herdr.zh-CN.md#其他机器)）；
在这边 `ssh -o BatchMode=yes <目标> true` 不用任何输入就能成功（密钥在 ssh-agent 里，或者你自己
在 `~/.ssh/config` 里配了 ControlMaster）；那台机器的 sshd 允许转发（默认允许）；那边有 `git`。

```sh
workriver machine add box --ssh me@box        # 你起的名字，加上 ssh 能用的目标
workriver machine enable box
workriver machine list                        # "connected, 3 agents"，或者连不上的原因
```

页面的设置里也能做同样的事。然后在那台机器上构建命令行，并确认它能通过这条连接找到这个服务：

```sh
git clone <这个仓库> && cd workriver && cargo build --release
ln -s "$PWD/target/release/workriver" ~/.local/bin/workriver    # PATH 里的任何位置
workriver status                              # 回答来自你这台机器上的服务
```

那边通往这个服务的端口每次连接都换一个：由 ssh 挑一个空闲的，workriver 把它写到那边的
`~/.config/workriver/port`，那边的 `workriver` 命令会读它（除非设了 `WORKRIVER_PORT`）。不用配置。
如果那边的运行时不在 workriver 找它的地方，`machine add` 可以指明它在哪里
（[runtimes/herdr.zh-CN.md](runtimes/herdr.zh-CN.md#其他机器)）。

那边的 agent 会带着机器名显示。它们和这边的 agent 在同样的工作区里工作：工作区没有机器之分。那边的
worker 通过拿到的任务在某个工作区里工作；那边的 agent 也可以像别的 agent 一样被你指定为工作区的管理者。

单元登记在干活的地方：那边的 agent 照常在那边登记；这边的管理者要指明机器，
并给出那边的路径和分支名（`workriver unit add --machine box --repo /home/me/app --branch feat/queue`）；
登记错了地方的单元用 `workriver unit move u1 --machine box --repo … --branch …` 改正。分支改了名、
或者变基到了新分支，也用这条命令（`workriver unit move u1 --branch feat/queue-2 [--base main-2]`）：单元、
任务和发现都留着，提交从新分支读，页面上写"之前在 feat/queue"，提交留在旧分支上的任务会如实标出来。
没写 `Task:` 尾注的提交可以事后关联到任务，由做这个任务的 agent、管理者或你来做：
`workriver commit link <hash> u1#3`，或者在未登记节点上的"它属于任务"里选；变基之后关联仍然有效。对那边的单元，workriver 通过 ssh 跑 git 读它的提交，
每 15 秒一次。那边附上的截图通过这条连接上传；发给那边 agent 的消息里给的是把每个文件取到
agent 当前文件夹的命令（`workriver attach fetch <id>.<ext>`；用 `curl` 访问服务会被拒绝）。终端按钮会在那台机器的运行时里聚焦到 agent 的终端；
还要你自己做什么，见 [runtimes/herdr.zh-CN.md](runtimes/herdr.zh-CN.md#其他机器)。

## 7. 更新

```sh
git pull
cd frontend/web && pnpm install && pnpm build && cd ../..
cargo build --release
```

然后停掉 `workriver serve`（Ctrl-C）再启动。数据会保留。

你跟踪的**每一台其他机器**上也要做同样的构建（在那边 `git pull`、`cargo build --release`）：
那边 agent 用的 `workriver` 命令要和服务是同一个版本。在这边和那边运行 `workriver status`，
都会显示服务的版本和提交号；如果运行它的命令是另一个构建，会明说。

在工作区成为 workriver 自己的记录之前写下的库，启动时会被拒绝（"holds events in the form
from before virtual workspaces"；库里什么也不改）。用仓库里已经不再带的脚本把它的一份副本
转换一次：提交 `7cdf576` 里有它，在 `scripts/migrate-virtual-workspaces/`，它的 `README.md`
写了怎么运行。在任何一台有克隆的机器上，把那个提交取到一个单独的 worktree 里（`git worktree
add --detach .local/migrate-script 7cdf576`），再照那里的 README 做。

Cargo 从不清理 `target/` 里的旧构建：更新几周后 `target/debug` 可能超过 20 GB。
`cargo clean --profile dev` 只删 `target/debug`，服务运行所用的 `target/release/workriver`
不受影响。每个 git worktree 有自己的 `target/`（构建并编译测试后约 1.5 GB），随 worktree
删除。worktree 有意不共用一个构建目录：cargo 按 checkout 内的路径给本仓库自己的 crate
命名，按文件时间决定要不要重编，所以文件比别的 worktree 上次构建更旧的 worktree
会跑到那个 worktree 的代码。

## 排错

- **"cannot listen on 127.0.0.1:47820"**：端口被另一个 workriver（或别的程序）占了。停掉它，
  或者用 `workriver serve --port 47821`，命令行也用同一个端口（`--port 47821` 或
  `WORKRIVER_PORT=47821`）。
- **某台机器显示 "not connected"**：`workriver machine list` 会原样打印 ssh 的报错。
  *Permission denied* 或 *Host key verification failed*：先让 `ssh -o BatchMode=yes <目标> true`
  能成功。*remote port forwarding failed*：你的 ssh 配置里给这台主机设的端口转发让 workriver 的
  连接失败了；给 workriver 一个不带这些转发的 Host 别名。*connected, but its … does not
  answer*：运行时在那边没有运行，或者它在别处（[runtimes/herdr.zh-CN.md](runtimes/herdr.zh-CN.md#排错)）。
- **有东西很慢**：服务在 `~/workriver-data/logs/` 里记一份计时日志（`perf.jsonl`，最多 5 个文件、
  每个 5 MB）。`workriver perf [--since 1h]` 给出汇总：最慢的请求、每次快照和每个单元花了多久、
  其他机器断开了多久。要连每一次 git 和 ssh 调用也记下来，用 `workriver serve --perf-detail`
  （或者 `WORKRIVER_PERF=detail`）启动服务。`workriver perf --bundle` 把汇总和最新的几行写进一个
  文件并打印路径：要发的就是这个文件。里面只有时间、次数、路由模板（`/api/task`，从不含查询参数）、
  单元 id 和机器名；从不包含提交说明、文件内容、任务文本、发现、补充说明、提示词、令牌、cookie 或
  路径（git 调用里路径的位置写成 `…`）。发之前先看一眼。
- **电脑睡眠或换了网络之后，某台机器变成"断开"**：这是预期的，会自己恢复。ssh 大约 30 秒内会
  发现连接已死（睡眠醒来后立刻发现），workriver 随即重连：先等 2 秒，之后越等越久，最多一分钟；
  电脑醒来时立刻重连。这期间不会有任何操作卡在那台机器上：它的单元和 agent 显示为上次看到的
  状态，标着"14:02 起断开"，`workriver machine list` 会给出最后一次的错误和下次重连的时间。每次
  连接在那边用一个新端口，所以那台机器的 sshd 还留着的死连接会话（它不检查客户端是否还活着，
  可能留半小时）挡不着任何事。那边的 `workriver` 命令几秒内就会放弃，提示 "cannot reach
  workriver … through the ssh tunnel (it may be reconnecting); try again shortly"，而不会卡住。
  如果那台机器的 sshd 归你管，可以让它更快丢掉这种会话：在 `/etc/ssh/sshd_config` 里设
  `ClientAliveInterval 15` 和 `ClientAliveCountMax 3`。不是必需的。想看一眼的话，在那边运行
  `ss -tnp | grep sshd`（每个连接一个会话，有的还有数据等着发给已经不在的对端）；它们会自己
  结束，也可以 `kill` 掉对应的 sshd 进程。
- **页面说连不上服务**：`workriver serve` 没在运行，或者跑在别的端口上。
- **`agent runtime: not connected`**，或者启动时有关于运行时的警告（`workriver status`
  会显示）：运行时没在运行，或者服务在别处找它；见 [runtimes/herdr.zh-CN.md](runtimes/herdr.zh-CN.md#排错)。
- 在 agent 的终端里用命令行时报 **"cannot tell which agent this is"**：服务连不上运行时（见上一条）。
  这期间要以你自己的身份运行命令，请用运行时之外的终端。
- **"workriver answers the user, and agents from their own …"（401）**：服务不确定这个命令或页面是你。页面：运行
  `workriver open`。命令行：在 workriver 所在的机器上，用一个不属于任何 agent 的终端运行。决定、
  手工步骤、杂事、项目、机器、工作区和管理者的席位只有你能动；见[安全模型](security.zh-CN.md)。
- 页面上显示 **"没送出：…"**（鼠标停上去能看到细节）：
  - *这个意图还没有管理者*：给它的工作区指定一个（该工作区那一行的"管理者"，或
    `workriver workspace manager <ws> <agent>`）；
  - *管理者没在运行*：在它原来运行的地方恢复它（它会自己重新成为管理者），或者指定另一个 agent；
  - *席位已经重新指定*：这条消息是发给之前的管理者的，不会转交；请重新写给管理者；
  - *连不上 …* 或 *… 没有接收*（页面上写的是运行时的名字）：见
    [runtimes/herdr.zh-CN.md](runtimes/herdr.zh-CN.md#排错)；先在终端里处理好，再用"复制消息"手动粘贴。
