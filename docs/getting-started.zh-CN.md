# 上手

[English](getting-started.md)

在一台装好 [mise](https://mise.jdx.dev)、git 和 [Herdr](https://herdr.dev)（0.9，协议 22）的
Mac 上，把 workriver 跑起来，并让每个 Herdr 工作区都有一个管理者 agent。

## 1. 构建

```sh
git clone https://github.com/path-pass/workriver.git
cd workriver
mise install                 # 按 mise.toml 装好 Rust 和 Node，只对这个目录生效
cd frontend/web && npm install && npm run build && cd ../..
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

在一个单独的 Herdr 窗格里启动它，这样它能找到 Herdr 的 socket，也会一直运行：

```sh
workriver serve              # http://127.0.0.1:47820
```

然后在一个不是 agent 窗格的终端里运行 `workriver open`：它用一个一次性的链接在浏览器里打开页面，
让这个浏览器以你的身份登录。这个链接不会被打印出来，因为 agent 读得到 Herdr 窗格；在没有浏览器的
机器上，`workriver open --print` 会打印一个只有一分钟有效的链接：请在 agent 读不到的终端里用。直接打开地址只会看到"怎么打开"
的说明；原因见[安全模型](security.zh-CN.md)。

打开 <http://127.0.0.1:47820>。`workriver status` 应该显示 `agent runtime: connected`。

- 数据：`~/workriver-data/workriver.db`（设 `WORKRIVER_HOME` 换位置）。只有一个 SQLite
  文件；备份时先停掉服务再复制它。附件（截图、报告）放在旁边的 `attachments/` 里，同样的
  内容只存一份，和记录一样永不删除；`workriver status` 会显示它们占了多少空间。
- 页面上的终端按钮会在 Herdr 里把 agent 的窗格切到前面，再把终端应用切到前台（默认
  Ghostty；用 `workriver serve --terminal-app iTerm` 改，传 `''` 表示不切）。

## 3. 项目和待办

**项目**是一个装着一个或多个仓库的目录。装了好几个仓库的目录要登记一下：
`workriver project add ~/code/shop`（它的仓库就是下一层带 `.git` 的子目录）。不在任何已登记
目录里的仓库自己就是一个项目，不用做什么。

每个项目有一份**待办**：它的**路线图**（计划中的意图，按你想要的顺序排的目标）和**延后的发现**
（先放一放的问题）。在页面左栏往路线图里加（Roadmap 旁的"+"，拖动调整顺序），或者：

```sh
workriver backlog add "Offline sync" --goal "Notes can be taken offline and merge later." --project ~/code/shop
workriver backlog list
```

**导入已有的清单。** 每份笔记文件的写法都不一样，所以没有固定的解析器；由一个 agent 用导入
skill 来做（`skills/workriver-backlog-import/`，或者把 `workriver guide import` 的输出贴给它）。
告诉它哪个文件、导进哪个项目；它读文件，分清哪些是目标、哪些是遗留的发现，先给你看一张预览表，
等你同意，再按文件里的顺序用 `workriver backlog add` 和 `workriver backlog add-finding` 加进去。
它不会改动原文件。`workriver backlog export --project P` 把待办再打印成 Markdown。

## 4. 给每个工作区一个管理者

一个 Herdr 工作区跑一个或多个意图，由它的管理者 agent 规划和派活。在每个工作区里启动一个
coding agent 当管理者，并把 skill 给它。

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

然后告诉它，例如："你是这个工作区的 workriver 管理者。开始路线图里的第一个意图，规划单元和
任务，交给 worker agent 去做。"页面上计划中的意图旁边的"开始"会列出同样的步骤和可复制的命令。

可选：如果 agent 用 Playwright MCP 截图，它的文件默认会落在项目里（`.playwright-mcp/`）。
可以把它自己命名的文件指到 workriver 的目录，例如 Claude Code：
`claude mcp add playwright -- npx @playwright/mcp@latest --output-dir ~/workriver-data/attachments/inbox/playwright`
（或者设 `PLAYWRIGHT_MCP_OUTPUT_DIR`）。这只管没有指定文件名的截图；agent 自己给了文件名的，
仍然相对项目目录保存。之后 agent 用 `workriver attach <文件> --task u1#3 --move` 把需要的附上。

## 5. 接下来会发生什么

管理者在自己的窗格里：

```sh
workriver manager here                 # 登记为这个工作区的管理者
workriver backlog list                 # 路线图：i1 "Mentions" ……
workriver intent start i1              # 在这里开始；成为这个工作区的当前意图
workriver unit add --title "Mentions" --branch feat/mention   # 单元自己的分支：feat/、fix/ 或 chore/ 开头
workriver task add u1 --title "Find the people mentioned in a message" --agent finder
herdr agent start finder --kind claude --pane <窗格 id>   # 见 `herdr --skill`
herdr agent prompt finder "<任务 u1#1、一句话目标、在哪里干、\"运行 workriver task show u1#1\"，以及 workriver guide worker 的全文>"
```

worker 自己记录工作：

```sh
workriver task show u1#1                          # 任务全文：要求、起因、补充说明
workriver task start u1#1
git commit -m "Find mentions" -m "Task: u1#1"     # 这一行 trailer 把提交和任务连起来
workriver task deliver u1#1 --evidence 'npm test => passed: 6 passed'
```

单元分支上没有 `Task:` 这一行的提交会显示为未登记，是一个要补上的缺口。
每个单元都标出自己的状态：进行中、等待中（有未决定的发现或 gate）、可以合并、已合并（快进或
合并提交 workriver 自己能看到；squash 合并之后要运行 `workriver unit merged u1`）、已结束
（`workriver unit done u1 --reason "……"`，用于不经合并就结束的工作）。一个意图的单元都已合并或
结束时，由管理者用 `workriver intent done i1` 结束这个意图。审查和测试是单独的
任务。它们找出的问题默认由这个意图的管理者决定（带机器人标记；你可以在页面上改掉任何一个决定，
也可以按意图把决定权留给自己：左栏的"发现由谁决定"，或者 `workriver intent set i1 --findings user`）。
归你决定的问题在页面的 **需要你** 里等着：在那里对每一个接受、按我的方式改、驳回或延后。延后的会留在**待办**面板里；勾选几条，
"一起做"会把它们作为一条消息发给这个意图的管理者，由它合成一个清理单元。你的决定和
手工步骤的结果会通过 Herdr 送到该送的 agent（接受的问题送给管理者，由它派活）；下面那一行
显示有没有送到、有没有被接手，"复制消息"能拿到同样的文字，用来手动粘贴。

## 6. 其他机器

一个 workriver 也能跟踪其他机器上的 agent。每台机器跑自己的 Herdr；workriver 和每台机器之间保持
一条 ssh 连接，把那台机器的 Herdr socket 带到这边，同时给那边的 `workriver` 命令行一条回到这个
服务的路。

要求：两边的 Herdr 都是 0.9.1 或更新（同一套 socket 协议），那台机器上的 Herdr 正在运行；
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
如果那边 Herdr 的 socket 不在 `~/.config/herdr/herdr.sock`
（用了命名 session），用 `--herdr-socket <路径>` 指明。

那边的 agent 会带着机器名显示。workriver 不假定任何 id 跨机器唯一；那边的工作区是独立的，即使和
这边的同名。要把两个工作区合成一个（一个管理者，同一批意图），就关联它们：

```sh
workriver workspace list                      # box:chat  not linked
workriver workspace link box:chat chat
```

或者在设置里选。单元登记在干活的地方：那边的 agent 照常在那边登记；这边的管理者要指明机器，
并给出那边的路径和分支名（`workriver unit add --machine box --repo /home/me/app --branch feat/queue`）；
登记错了地方的单元用 `workriver unit move u1 --machine box --repo … --branch …` 改正。分支改了名、
或者变基到了新分支，也用这条命令（`workriver unit move u1 --branch feat/queue-2 [--base main-2]`）：单元、
任务和发现都留着，提交从新分支读，页面上写"之前在 feat/queue"，提交留在旧分支上的任务会如实标出来。
没写 `Task:` 尾注的提交可以事后关联到任务，由做这个任务的 agent、管理者或你来做：
`workriver commit link <hash> u1#3`，或者在未登记节点上的"它属于任务"里选；变基之后关联仍然有效。对那边的单元，workriver 通过 ssh 跑 git 读它的提交，
每 15 秒一次。那边附上的截图通过这条连接上传；发给那边 agent 的消息里给的是链接
（`http://127.0.0.1:$(cat ~/.config/workriver/port)/api/attachments/…`）。终端按钮会在那台机器的 Herdr 里聚焦到对应窗格；
在 Herdr 侧栏里切到那台机器要你自己来。

## 7. 更新

```sh
git pull
cd frontend/web && npm install && npm run build && cd ../..
cargo build --release
```

然后停掉 `workriver serve`（Ctrl-C）再启动。数据会保留。

你跟踪的**每一台其他机器**上也要做同样的构建（在那边 `git pull`、`cargo build --release`）：
那边 agent 用的 `workriver` 命令要和服务是同一个版本。在这边和那边运行 `workriver status`，
都会显示服务的版本和提交号；如果运行它的命令是另一个构建，会明说。

## 排错

- **"cannot listen on 127.0.0.1:47820"**：端口被另一个 workriver（或别的程序）占了。停掉它，
  或者用 `workriver serve --port 47821`，命令行也用同一个端口（`--port 47821` 或
  `WORKRIVER_PORT=47821`）。
- **某台机器显示 "not connected"**：`workriver machine list` 会原样打印 ssh 的报错。
  *Permission denied* 或 *Host key verification failed*：先让 `ssh -o BatchMode=yes <目标> true`
  能成功。*remote port forwarding failed*：你的 ssh 配置里给这台主机设的端口转发让 workriver 的
  连接失败了；给 workriver 一个不带这些转发的 Host 别名。*connected, but its Herdr does not
  answer*：那边没有运行 Herdr，或者 socket 在别处（`--herdr-socket`）。
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
- **`agent runtime: not connected`**，或者启动时有关于 Herdr 的警告（`workriver status`
  会显示）：Herdr 没在运行，或者服务看的是另一个 socket。在 Herdr 窗格里启动服务，或者设
  `HERDR_SOCKET_PATH`。协议警告表示 Herdr 不是 0.9；可能还能用，但没测过。
- 在窗格里用命令行时报 **"cannot tell which agent this is"**：服务连不上 Herdr（见上一条）。
  这期间要以你自己的身份运行命令，请用 Herdr 之外的终端。
- **"workriver answers the user, and agents from their own Herdr pane…"（401）**：服务不确定这个命令或页面是你。页面：运行
  `workriver open`。命令行：在 workriver 所在的机器上，用一个不是 agent 窗格的终端运行。决定、
  手工步骤、杂事、项目、机器和工作区关联只有你能做；见[安全模型](security.zh-CN.md)。
- 页面上显示 **"没送出：…"**（鼠标停上去能看到细节）：
  - *这个意图还没有管理者*：在这个意图开始时所在工作区的管理者窗格里运行 `workriver manager here`；
  - *管理者没在运行*：重新启动它，再登记一次；
  - *连不上 Herdr*：见上面；
  - *Herdr 没有接收*：通常是 `agent_not_ready`（agent 还在启动，或者不是窗格的前台程序）或
    `agent_blocked`（它停在一个提问或确认框上）。先在终端里处理好，再用"复制消息"手动粘贴。
