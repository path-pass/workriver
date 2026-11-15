# workriver

[English](../README.md)

在很多 coding agent 同时干活的时候，仍然知道发生了什么。

同时跑二十个 agent 会话时，没有人能盯住每一个。workriver 在工作发生的时候把它记下来，并且画成一张图：

- **意图和单元**：意图是一个目标，拆成若干个可以各自独立实现的单元。
- **活动**：在一个单元里，每一份交给 agent（或者交给你，作为手工步骤）的任务是一个节点，有输入和输出。任务连成一条流，在审查提出问题时分叉，之后再汇合。
- **代码**：这个单元碰过的文件，左边是文件树，右边是改动。
- **需要你**：跨所有工作的一条队列：agent 在等你做的手工步骤、要你接受驳回或延后的问题、没写任务编号的提交这类缺口。

它在你自己的机器上作为一个服务运行。agent 在自己的 shell 里运行 `workriver` 命令行来上报工作；谁在运行、在哪里，从 [Herdr](https://herdr.dev) 取（还没接上）；你在浏览器里看。

## 状态

刚开始。服务能记录意图、单元、任务、问题和杂事，从 git 读出各单元的提交，并让网页界面显示真实数据。把你的决定送到 agent、接上 Herdr 是下一步。

## 构建和运行

运行时由 [mise](https://mise.jdx.dev) 管理（`mise.toml`：Rust 和 Node）。网页界面会打进二进制文件里，所以先构建界面：

```
cd frontend/web && npm install && npm run build && cd ../..
cargo build --release
./target/release/workriver serve          # http://127.0.0.1:47820
```

数据在 `~/workriver-data/workriver.db`（用 `WORKRIVER_HOME` 换位置）。`workriver --help` 说明各个命令；提交信息里写一行 `Task: u1#3`，这个提交就归到那个任务。

改界面时：在 `frontend/web` 里 `npm run dev` 连正在运行的 `workriver serve`；`npm run dev:mock` 改用示例数据。

## 许可

[MIT](../LICENSE)
