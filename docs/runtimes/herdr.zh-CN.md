# Herdr

[English](herdr.md)

workriver 通过一个 agent 运行时得知谁在运行、在哪里、处于什么状态，并把消息送到 agent 那里。
目前这个运行时是 [Herdr](https://herdr.dev)，也是 workriver 唯一支持的一个。其他文档里属于
Herdr 自己的内容都放在这一页：[README](../README.zh-CN.md)、[上手](../getting-started.zh-CN.md)
和[安全模型](../security.zh-CN.md)里说"运行时"的地方，指的就是 Herdr；说 agent 的终端的地方，
指的是它所在的 Herdr 窗格。工作区不是 Herdr 的：workriver 的工作区是它自己的记录（`ws1`），
Herdr 工作区只是窗格所在的地方。

## 安装和运行

workriver 所在的机器上需要 Herdr 0.9（协议 22）。

在一个单独的 Herdr 窗格里启动服务，这样它能找到 Herdr 的 socket，也会一直运行：

```sh
workriver serve              # http://127.0.0.1:47820
```

Herdr 的 socket 是 `~/.config/herdr/herdr.sock`（或 `HERDR_SOCKET_PATH`）；没有 Herdr 时服务
照常记录和显示，只是看不到 agent。`workriver status` 应该显示 `agent runtime: connected`。

页面上的终端按钮会在页面里以只读方式打开 agent 的终端；终端视图里的"在终端应用中打开"会在 Herdr 里
把 agent 的窗格切到前面，再把终端应用切到前台。

## 在页面上回应卡住的 agent

这件事用到 Herdr 的以下部分，都经过同一个 socket：

- **发现。** 服务仍然每 2 秒读一次所有 agent 的状态。一条长连接 `events.subscribe`（订阅
  `pane.agent_detected`、`pane.closed`，以及每个 agent 窗格的 `pane.agent_status_changed`——Herdr 只接受
  逐个窗格订阅）让状态一变就立刻读一次；检测到新窗格或窗格关闭时，用当时的窗格列表重新连接。
- **Herdr 漏掉的确认框。** Herdr 0.9.5 把 Codex 的 "Hooks need review" 报成 idle，把 "Implement this
  plan?" 报成 done。agent 变成 idle 或 done 时（以及至少半个周期后的第一次轮询再读一次，中间被状态变化或读屏结果提前唤醒的轮询不算），以及往任何窗格输入消息之前，服务都会读
  窗格的屏幕：有选项、下方又提示了按键的，agent 就算"屏幕上在等"，和 blocked 的一样计数、消息一样先留着。
  其他机器上的窗格在轮询之外读，每个 agent 同时只读一次，所以慢的机器不会拖住本机的状态；读到的结果在读完时显示，只有改变了标记的读屏才会唤醒轮询。
- **门。** `agent.list`（通过 session 快照）把一个会话解析到它唯一的窗格；`pane.read`（source
  `visible`，纯文本）就是屏幕；`agent.explain` 给出 Herdr 判定 blocked 的原因，workriver 只保留命中规则的 id 和这条规则自己的固定
  短语（`bash_permission_prompt: …`），是 Herdr 的话，不是屏幕上的：Herdr 从屏幕上读出的
  证据预览不保留，因为门里显示的就是屏幕本身；
  `pane.send_keys` 发键，用 Herdr 的键名（`up`、`down`、`enter`、`esc`、`tab`、数字）；消息用
  `agent.prompt` 发到那个窗格（已指定的管理者发到它固定的窗格，不再重新解析），发之前重新读一次
  `agent.list`，确认那个窗格仍显示这个会话。
- **终端视图。** Herdr 在它的 client socket 上推送窗格的终端画面：`herdr.sock` 旁边的
  `herdr-client.sock`（socket 是 `X.sock` 时就是 `X-client.sock`），协议是它自己的；workriver 为此运行
  Herdr 自己的命令行 `herdr terminal session observe <窗格>`，每个打开的视图一个，`HERDR_SOCKET_PATH` 设为它
  所用的 socket，并清掉它自己终端里的窗格变量。所以服务 `PATH` 上的 `herdr` 必须和 Herdr 服务端同一版本。
  视图是只读的，从不改变窗格的尺寸。

## 工作区和管理者

工作区是 workriver 自己的记录，由你在某个项目里建立；Herdr 工作区只是一组窗格。Herdr 工作区的任何
东西都不能认出 workriver 的工作区：Herdr 重启后会把已关闭工作区的 id 再发一次，两个工作区可以同名，
任何程序都能通过 socket 给它们改名、新建、关闭。所以窗格怎么摆随你（一个 workriver 工作区对应一个
Herdr 工作区看起来清楚，但不是要求）：workriver 只在 agent 旁边显示它的窗格在哪个 Herdr 工作区，并把
临时 agent 的窗格开在它服务的那个 agent 旁边。

工作区的管理者由你自己给，有两种方式。从页面启动（该工作区那一行的"启动管理者"）：workriver 在与
workriver 工作区同名的 Herdr 工作区里（取第一个同名的，没有就新建）新开一个标签页，不抢焦点，目录是项目
目录；读出它的 shell（`pane.process_info`）；记下这次启动；然后在那里调用 Herdr 的 `agent.start`，带上
harness 的类型和以 Herdr 列表形式给出的参数（只在你选了模型时带 `--model <模型>`），名字是这次启动自己的
`<ws>-manager-<n>`（`n` 是这次启动在 workriver 记录里的编号）：Herdr 的整个服务里一个正在运行的名字只能用一次
（`agent_name_taken`），而之前启动的 agent 可能还在运行。Herdr 报告这个窗格里的 agent 已就绪（idle 或 done）、
有 harness 自己的会话，并且屏幕上没有以 `⚠` 开头的行时，workriver 才指定它为管理者；Claude Code 和 Codex
启动时遇到不认识的模型就是这样报告的（两者照样启动、照样就绪）。屏幕上有这样一行的启动不指定任何人就结束，
agent 留在窗格里继续运行，页面上显示那一行。Herdr 的 `agent_not_ready`
（harness 启动时被挡住，比如在新目录里问是否信任）会保留窗格：在窗格里或从"需要你"里回答。哪些 harness
可选来自 Herdr 的 `integration.list`：命令找得到，且 Herdr 对它的集成是最新的（`herdr integration status`；
`herdr integration install claude` 安装）。在 Herdr 0.9.5 上实测：通过 socket 调用的
`agent.start` 会立刻返回，启动仍在进行；窗格里的 shell 还没到提示符时会被拒绝（`agent_pane_busy`），
workriver 会再试一阵，然后像 `herdr agent start` 那样等待（就绪，或被挡住：未就绪）。Herdr 输入参数时会
加引号。agent 的名字只能用小写字母、数字、`-` 和 `_`。或者自己在一个窗格里启动 coding agent，然后在页面上选它
（该工作区那一行的"管理者"），或者在没有 agent 的窗格、或 Herdr 之外的终端里运行
`workriver workspace manager ws1 <agent>`。
Herdr 重启后所有窗格都是新的：在同一个文件夹里恢复的管理者会自己重新成为管理者；不是这样的，在
"需要你"里等你点一下。

管理者用 Herdr 自己的命令启动干活的 agent、把任务交给它（见 `herdr --skill`）：

```sh
herdr agent start finder --kind claude --pane <窗格 id>   # 见 `herdr --skill`
herdr agent prompt finder "<任务 u1#1、一句话目标、在哪里干、\"运行 workriver task show u1#1\"，以及 workriver guide worker 的全文>"
```

## 其他机器

每台机器跑自己的 Herdr；workriver 和每台机器之间保持一条 ssh 连接，把那台机器的 Herdr socket
带到这边，同时给那边的 `workriver` 命令行一条回到这个服务的路。

要求：两边的 Herdr 都是 0.9.1 或更新（同一套 socket 协议），那台机器上的 Herdr 正在运行。

如果那边 Herdr 的 socket 不在 `~/.config/herdr/herdr.sock`（用了命名 session），用
`--herdr-socket <路径>` 指明。

这条连接也把那台机器的 client socket 带到这边（数据目录里的 `herdr-<名字>-client.sock`，来自那边 Herdr
socket 旁边的 `-client.sock`），所以页面上对那台机器上 agent 的终端视图和本机一样能用；没有它时视图会说明，
门照样能回应（它只需要读屏和按键）。这个 socket 的路径比 API socket 长 7 个字节，而 Herdr 只会在 API socket
旁边找它：路径长到放不进 socket（macOS 104 字节，Linux 108 字节；数据目录或机器名很长时）就不转发它，这台机器
照常跟踪，只有它的终端视图会说明原因。只测过本机回环的 ssh，没有测过经网络的第二台机器。

"在终端应用中打开"会在那台机器的 Herdr 里聚焦到对应窗格；在 Herdr 侧栏里切到那台机器要你自己来。

## Herdr 下的安全

- 命令行会报上自己所在的窗格（`HERDR_PANE_ID`）；服务只拿它和自己查出的进程所在窗格核对。
  Herdr 告诉服务每个窗格根上的 shell。
- Herdr 窗格 agent 读得到：所以服务从不打印任何能登录的东西，`workriver open`，尤其是
  `workriver open --print`，要在 agent 读不到的终端里运行。
- 你从页面运行的命令，会被输入到单元所在机器上名为 "workriver-commands" 的 Herdr 工作区里、
  一个新标签页中的普通 shell（没有 agent）。另一台机器上的命令在那台机器的 Herdr 里、以你在那里
  的身份运行。
- **Herdr 自己的 socket 属于你的账户。** 任何程序都能告诉 Herdr 某个窗格里是哪个 agent，也能关掉
  一个窗格。程序可以通过 Herdr 关掉一个会话最早出现的窗格，再报上它的会话。用 `herdr agent start`
  启动的 agent 不是 Herdr 只凭上报才认作 agent 的窗格。
- **别的终端。** Herdr 允许任何 agent 读任何窗格的屏幕、往里面打字；别的 agent 终端上显示的内容，
  workriver 藏不了。workriver 自己读窗格屏幕、推送终端画面、在窗格里按键的路由只有你能用（见
  [安全模型](../security.zh-CN.md)）。

## 排错

- **`agent runtime: not connected`**，或者启动时有关于 Herdr 的警告（`workriver status`
  会显示）：Herdr 没在运行，或者服务看的是另一个 socket。在 Herdr 窗格里启动服务，或者设
  `HERDR_SOCKET_PATH`。协议警告表示 Herdr 不是 0.9；可能还能用，但没测过。
- **某台机器显示 *connected, but its Herdr does not answer***：那边没有运行 Herdr，或者 socket
  在别处（`--herdr-socket`）。
- **终端视图说打不开**：client socket 不在（Herdr 不提供终端画面流，或者这台机器在这个版本之前就连上了：停用再
  启用它），它在这边的路径对 socket 来说太长（换短一点的数据目录或机器名），或者服务 `PATH` 上没有 `herdr`、版本和服务端不同。
- 页面上显示 **"没送出：…"**：
  - *连不上 Herdr*：见第一条；
  - *Herdr 没有接收*：通常是 `agent_not_ready`（agent 还在启动，或者不是窗格的前台程序）或
    `agent_blocked`（它停在一个提问或确认框上）。先在终端里处理好，再用"复制消息"手动粘贴。
