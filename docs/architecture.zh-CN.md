# 代码怎么组织

[English](architecture.md)

给想在 workriver 源码里找路的人的一张简短地图。完整的目标目录树（每个文件一行）在
`design/restructure.md`（第 2 节）。

workriver 是一个 Rust 程序（服务和 CLI 在同一个可执行文件里）加一个网页；网页构建成静态文件，
由服务提供。每一部分都按领域切分：同一件事在每一层用同一个文件夹名（planning、work、
conversations……）。

## 各部分

| 位置 | 是什么 |
|---|---|
| `crates/workriver-core/` | workriver 记录什么、按什么规则记录，不碰网络、存储和界面：`records/`（事件和它们累加成的状态）、`planning/`（项目、意图、单元）、`work/`（任务、问题发现、关卡、杂务）、`conversations/`（提问、消息、附件的记录）、`access/`（谁能做什么、盲审的视图）、`runtime.rs`（运行时接口，用 workriver 自己的词）、`ports/`（数据端口：服务对保存记录的一方提出的要求） |
| `crates/workriver-data/` | 事件库：一个只追加的 SQLite 文件（`sqlite/`）。它实现 core 的数据端口，服务只通过这个端口使用它 |
| `crates/workriver-scan/` | 读 git 和文件树，本机或经 ssh：`runner.rs`、`history/`、`files/` |
| `crates/workriver/src/cli/` | `workriver` 命令：解析参数、证明调用者是谁、请求服务、打印结果 |
| `crates/workriver/src/server/` | 服务：`app/`（启动、记录、轮询）、`access/`（调用者是谁、能做什么、能看什么）、`http/`（路由和请求守卫），然后每个领域一个文件夹（`planning/`、`work/`、`conversations/`、`commands/`、`machines/`、`files/`、`views/`、`delivery/`） |
| `crates/workriver/src/runtime/` | agent 运行时，藏在一个门面后面：`follow.rs`（把多台机器当成一个）、`ssh.rs`（隧道），每种运行时一个文件夹；目前只有 `herdr/`（[Herdr 运行时](runtimes/herdr.zh-CN.md)） |
| `crates/workriver/src/common/` | 服务和 CLI 共用的东西：设置（`config/`）、数据目录里的文件（`home.rs`）、进程，以及服务、CLI 和网页共同认定的 JSON 结构（`wire/`） |
| `frontend/web/src/` | 网页：`app/`（外壳）、`api/`（通往服务的唯一出口）、`workspace/`（载入快照），每个领域一个文件夹（`planning/`、`work/`、`attention/`、`agents/`、`conversations/`、`code/`、`attachments/`、`commands/`、`machines/`、`session/`），然后是共用部分（`content/`、`ui/`、`lib/`、`config/`、`i18n/`）和 `mock/`（示例数据） |
| `skills/` | agent 读的东西，编进可执行文件（`workriver guide`）：管理者的职责、每种情况一个做法（分组放在 `workriver-howto/references/`）、待办导入 |
| `docs/` | 给人看的文档（本文、上手、安全、运行时），以及 `design/` 里 agent 照着做的规格 |

## 依赖朝哪个方向

```text
workriver（可执行文件） ──► workriver-core
                        ──► workriver-data ──► workriver-core
                        ──► workriver-scan
workriver-core 和 workriver-scan 不依赖我们自己的任何 crate。

可执行文件内部：
  cli/    ──► runtime/（门面） ──► runtime/herdr/   （只有 runtime/ 看得到 herdr/）
  server/ ──► runtime/
  cli/、server/、runtime/ ──► common/
  cli/ 和 server/ 互不引用（服务的测试借用 CLI 的客户端）；
  runtime/ 两者都不引用；
  common/ 三者都不引用。

网页（frontend/web/src）：
  app/ ──► 各领域文件夹 ──► content/、ui/、lib/
  只有 api/ 和服务通信（存储文件自身的地址除外，在显示它的地方读取）
```

核心不提任何运行时：Herdr 自己的东西（它的话、它的 socket、它的位置）都留在
`runtime/herdr/` 里；有一个测试（`crates/workriver/tests/boundaries.rs`）在它的名字出现在
源码其他地方时失败，替身测试除外。所以可以在它旁边加另一种运行时，而不必动核心、服务的各领域
或网页。
