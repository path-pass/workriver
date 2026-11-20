# Herdr

[English](herdr.md)

workriver 通过一个 agent 运行时得知谁在运行、在哪里、处于什么状态，并把消息送到 agent 那里。
目前这个运行时是 [Herdr](https://herdr.dev)，也是 workriver 唯一支持的一个。其他文档里属于
Herdr 自己的内容都放在这一页：[README](../README.zh-CN.md)、[上手](../getting-started.zh-CN.md)
和[安全模型](../security.zh-CN.md)里说"运行时"的地方，指的就是 Herdr；说 agent 的终端的地方，
指的是它所在的 Herdr 窗格；工作区就是 Herdr 工作区。

## 安装和运行

workriver 所在的机器上需要 Herdr 0.9（协议 22）。

在一个单独的 Herdr 窗格里启动服务，这样它能找到 Herdr 的 socket，也会一直运行：

```sh
workriver serve              # http://127.0.0.1:47820
```

Herdr 的 socket 是 `~/.config/herdr/herdr.sock`（或 `HERDR_SOCKET_PATH`）；没有 Herdr 时服务
照常记录和显示，只是看不到 agent。`workriver status` 应该显示 `agent runtime: connected`。

页面上的终端按钮会在 Herdr 里把 agent 的窗格切到前面，再把终端应用切到前台。

## 工作区和管理者

一个 Herdr 工作区跑一个或多个意图；每个 Herdr 工作区有一个管理者 agent 负责规划。一个工作区的
管理者在自己的窗格里用 `workriver manager here` 登记，每个工作区一个。

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

终端按钮会在那台机器的 Herdr 里聚焦到对应窗格；在 Herdr 侧栏里切到那台机器要你自己来。

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
  workriver 藏不了。

## 排错

- **`agent runtime: not connected`**，或者启动时有关于 Herdr 的警告（`workriver status`
  会显示）：Herdr 没在运行，或者服务看的是另一个 socket。在 Herdr 窗格里启动服务，或者设
  `HERDR_SOCKET_PATH`。协议警告表示 Herdr 不是 0.9；可能还能用，但没测过。
- **某台机器显示 *connected, but its Herdr does not answer***：那边没有运行 Herdr，或者 socket
  在别处（`--herdr-socket`）。
- 页面上显示 **"没送出：…"**：
  - *连不上 Herdr*：见第一条；
  - *Herdr 没有接收*：通常是 `agent_not_ready`（agent 还在启动，或者不是窗格的前台程序）或
    `agent_blocked`（它停在一个提问或确认框上）。先在终端里处理好，再用"复制消息"手动粘贴。
