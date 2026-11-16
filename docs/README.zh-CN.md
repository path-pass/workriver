# workriver

[English](../README.md)

在很多 coding agent 同时干活的时候，仍然知道发生了什么。

同时跑二十个 agent 会话时，没有人能盯住每一个。workriver 在工作发生的时候把它记下来，并且画成一张图：

- **项目、意图和单元**：项目是装着仓库的目录；意图是其中的一个目标，开始之前排在项目的路线图里，开始之后拆成若干个可以各自独立实现的单元。
- **活动**：在一个单元里，每一份交给 agent（或者交给你，作为手工步骤）的任务是一个节点，有输入和输出。任务连成一条流，在审查提出问题时分叉，之后再汇合。
- **代码**：这个单元碰过的文件，左边是文件树，右边是改动。
- **需要你**：跨所有工作的一条队列：agent 在等你做的手工步骤、要你接受驳回或延后的问题、没写任务编号的提交这类缺口。
- **待办**：每个项目延后的发现，可以勾几条一起做，或者放弃。

它在你自己的机器上作为一个服务运行。agent 在自己的 shell 里运行 `workriver` 命令行来上报工作；谁在运行、在哪里，从 [Herdr](https://herdr.dev) 取；你在浏览器里看。

## 状态

第一个版本，可以用了。服务能记录意图、单元、任务、问题和杂事，从 git 读出各单元的提交，跟踪在 Herdr 里运行的 agent（每个 Herdr 工作区一个管理者 agent，跑一个或多个意图），维护每个项目的路线图和待办，把你的决定和手工步骤的结果送到该送的 agent，并让网页界面显示真实数据。其他机器上的 agent（每台机器跑自己的 Herdr）可以通过 ssh 跟踪；这部分是新的，目前只用替身测试过。

## 构建和运行

从一台新 Mac 到每个 Herdr 工作区都有管理者 agent，一步一步：[上手](getting-started.zh-CN.md)。

运行时由 [mise](https://mise.jdx.dev) 管理（`mise.toml`：Rust 和 Node）。网页界面会打进二进制文件里，所以先构建界面：

```
cd frontend/web && npm install && npm run build && cd ../..
cargo build --release
./target/release/workriver serve          # http://127.0.0.1:47820
```

数据在 `~/workriver-data/workriver.db`（用 `WORKRIVER_HOME` 换位置）。`workriver --help` 说明各个命令；提交信息里写一行 `Task: u1#3`，这个提交就归到那个任务。在 Herdr 的窗格里运行时，命令行知道是哪个 agent 在用它；一个工作区的管理者用 `workriver manager here` 登记，`workriver guide manager` / `workriver guide worker` 打印给 agent 的说明。Herdr 的 socket 是 `~/.config/herdr/herdr.sock`（或 `HERDR_SOCKET_PATH`）；没有 Herdr 时服务照常记录和显示，只是看不到 agent。

改界面时：在 `frontend/web` 里 `npm run dev` 连正在运行的 `workriver serve`；`npm run dev:mock` 改用示例数据。

## 许可

[MIT](../LICENSE)
