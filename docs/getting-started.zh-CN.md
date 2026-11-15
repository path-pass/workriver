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

打开 <http://127.0.0.1:47820>。`workriver status` 应该显示 `agent runtime: connected`。

- 数据：`~/workriver-data/workriver.db`（设 `WORKRIVER_HOME` 换位置）。只有一个 SQLite
  文件；备份时先停掉服务再复制它。
- 页面上的终端按钮会在 Herdr 里把 agent 的窗格切到前面，再把终端应用切到前台（默认
  Ghostty；用 `workriver serve --terminal-app iTerm` 改，传 `''` 表示不切）。

## 3. 给每个工作区一个管理者

一个 Herdr 工作区就是一个意图（一个目标）。在每个工作区里启动一个 coding agent 当管理者，
并把 `skills/workriver-manager/` 里的管理者 skill 给它：

- **Claude Code**：把这个目录链接进 skills 目录，只给一个项目或给所有项目：
  `ln -s "$PWD/skills/workriver-manager" <项目>/.claude/skills/workriver-manager`，或
  `ln -s "$PWD/skills/workriver-manager" ~/.claude/skills/workriver-manager`。
- **Codex**：同样链接到 `<项目>/.agents/skills/` 或 `~/.agents/skills/`。
- **其他工具**：运行 `workriver guide manager`，把输出贴进给它的第一条消息。

然后告诉它，例如："你是这个工作区的 workriver 管理者。目标：让聊天里的人能互相 @。
规划单元和任务，交给 worker agent 去做。"

## 4. 接下来会发生什么

管理者在自己的窗格里：

```sh
workriver manager here                 # 登记自己；意图以工作区的名字命名
workriver unit add --title "Mentions" --branch mention
workriver task add u1 --title "Find the people mentioned in a message" --agent finder
herdr agent start finder --kind claude --pane <窗格 id>   # 见 `herdr --skill`
herdr agent prompt finder "<任务 u1#1、它的目标，以及 workriver guide worker 的全文>"
```

worker 自己记录工作：

```sh
workriver task start u1#1
git commit -m "Find mentions" -m "Task: u1#1"     # 这一行 trailer 把提交和任务连起来
workriver task deliver u1#1 --evidence 'npm test => passed: 6 passed'
```

单元分支上没有 `Task:` 这一行的提交会显示为未登记，是一个要补上的缺口。审查和测试是单独的
任务；它们找出的问题在页面的 **需要你** 里等着。在那里对每一个接受、驳回或延后。你的决定和
手工步骤的结果会通过 Herdr 送到该送的 agent（接受的问题送给管理者，由它派活）；下面那一行
显示有没有送到、有没有被接手，"复制消息"能拿到同样的文字，用来手动粘贴。

## 5. 更新

```sh
git pull
cd frontend/web && npm install && npm run build && cd ../..
cargo build --release
```

然后停掉 `workriver serve`（Ctrl-C）再启动。数据会保留。

## 排错

- **"cannot listen on 127.0.0.1:47820"**：端口被另一个 workriver（或别的程序）占了。停掉它，
  或者用 `workriver serve --port 47821`，命令行也用同一个端口（`--port 47821` 或
  `WORKRIVER_PORT=47821`）。
- **页面说连不上服务**：`workriver serve` 没在运行，或者跑在别的端口上。
- **`agent runtime: not connected`**，或者启动时有关于 Herdr 的警告（`workriver status`
  会显示）：Herdr 没在运行，或者服务看的是另一个 socket。在 Herdr 窗格里启动服务，或者设
  `HERDR_SOCKET_PATH`。协议警告表示 Herdr 不是 0.9；可能还能用，但没测过。
- 在窗格里用命令行时报 **"cannot tell which agent this is"**：服务连不上 Herdr（见上一条）。
  `--as <名字>` 可以用你给的名字记录。
- 页面上显示 **"没送出：…"**（鼠标停上去能看到细节）：
  - *这个意图还没有管理者*：在管理者的窗格里运行 `workriver manager here`；
  - *管理者没在运行*：重新启动它，再登记一次；
  - *连不上 Herdr*：见上面；
  - *Herdr 没有接收*：通常是 `agent_not_ready`（agent 还在启动，或者不是窗格的前台程序）或
    `agent_blocked`（它停在一个提问或确认框上）。先在终端里处理好，再用"复制消息"手动粘贴。
