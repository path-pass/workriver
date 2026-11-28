# 代码怎么组织

[English](architecture.md)

给想在 workriver 源码里找路的人的一张简短地图。完整的目录树（每个文件一行）在
`design/layout.md`（写给 agent 的，英文）。

workriver 的服务、CLI 和每个 agent 的内部宿主进程使用同一个 Rust 可执行文件。网页构建成
静态文件，由服务提供。planning、work、conversations 等领域在各层使用相同的文件夹名。

## 各部分

| 位置 | 是什么 |
|---|---|
| `crates/workriver-core/` | workriver 记录什么、按什么规则记录，不碰网络、存储和界面：`records/`（事件和它们累加成的状态）、`planning/`（项目、意图、单元，以及放置单元步骤的模板）、`work/`（任务、问题发现、关卡、杂务）、`conversations/`（提问、消息、附件的记录）、`choices/`（问题的选项按记录的样子：A2UI v1.0 的一个固定子集，这里只有它的写入规则，core 不画任何东西）、`access/`（谁能做什么、盲审的视图）、`runtime.rs`（运行时接口，用 workriver 自己的词）、`screens/`（agent 终端上显示的内容：从中读出的确认框，workriver 能按的键）、`ports/`（数据端口：服务对保存记录的一方提出的要求） |
| `crates/workriver-data/` | 事件库：一个只追加的 SQLite 文件（`sqlite/`）。它实现 core 的数据端口，服务只通过这个端口使用它 |
| `crates/workriver-scan/` | 读 git 和文件树，本机或经 ssh：`runner.rs`、`history/`、`files/` |
| `crates/workriver/src/cli/` | `workriver` 命令：解析参数、证明调用者是谁、请求服务、打印结果 |
| `crates/workriver/src/server/` | 服务：`app/`（启动、记录、轮询）、`access/`（调用者是谁、能做什么、能看什么）、`http/`（路由和请求守卫）、`schedule/`（按时钟运行的 job：各自的定时器、一次只跑一个、上次结果），然后每个领域一个文件夹（`planning/`（包括模板的 YAML）、`work/`、`conversations/`、`commands/`、`terminals/`（在页面上读 agent 的终端、按键、看终端视图）、`machines/`、`files/`、`views/`、`delivery/`） |
| `crates/workriver/src/runtime/` | agent 运行时的门面包括 `follow.rs`（把多台机器当成一个）、`ssh.rs`（隧道）、`herdr/`（[Herdr 运行时适配器](runtimes/herdr.zh-CN.md)）和 `acp/`（[Codex 宿主进程及其日志](design/hosted-agent.md)，英文） |
| `crates/workriver/src/common/` | 服务和 CLI 共用的东西：设置（`config/`）、数据目录里的文件（`home.rs`）、进程，以及服务、CLI 和网页共同认定的 JSON 结构（`wire/`） |
| `crates/workriver/tests/hosted.rs` | 宿主进程的跨进程测试，使用 `tests/__fixtures__/hosted_app_server.rs` 模拟 Codex app-server |
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
  cli/    ──► runtime/（门面） ──► runtime/herdr/
  server/ ──► runtime/
  main.rs（host 命令） ──► runtime/ ──► runtime/acp/
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
