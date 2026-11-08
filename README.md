# OIJudger

安全、高效的 OI 本地评测工具（Windows）。目录结构与 LemonLime 兼容，可直接导入 LemonLime 的 `.cdf` 比赛。

- **沙箱隔离**：选手程序运行在无任何能力的 AppContainer 中——内核层面断网（含本机回环与 DNS），只能读写自己的临时目录，不能读取标准答案、其他选手文件或用户文件，不能创建子进程、读剪贴板；编译过程同样在沙箱内（`#include` 外部文件会失败）。无需管理员权限。
- **资源限制**：内存由 Job Object 在内核强制；按 CPU 时间判超时，墙钟兜底防止 `sleep`/阻塞；输出大小限制；栈空间自动设为内存限制（Windows 默认 2 MB 栈会让深递归爆栈）。
- **高效**：多线程并行评测（默认绑定到性能核，计时稳定）；编译与评测同时进行（编译器只使用评测线程之外的核心，不影响计时）；预编译 `bits/stdc++.h`；编译结果缓存，程序直接从缓存运行，避免杀毒软件重复扫描；增量评测（只评测源程序、数据或设置发生变化的部分）。
- **题型**：传统题（文件 / 标准 IO）、提交答案题、交互题（NOI 风格 grader，或 testlib 交互器双进程）、通信题（多源文件合编，或多程序：由 grader 启动各选手程序）。
- **判分**：多种比较方式、testlib 校验器、Lemon 6 参数校验器、测试点内多组数据取最低分、子任务依赖。
- **分析**：分数分布（含前缀和/后缀和）、平均分、中位数、标准差、区分度、测试点通过率、试题相关性、按 `contestants.csv` 分组对比；代码查重（忽略变量名、注释与格式）。
- **导出**：HTML（含/不含详细结果）、Excel、CSV。
- **在线比赛（局域网）**：评测机开启网页服务，选手在浏览器中看题、提交并实时得到结果；页面结构与交互逻辑同 DOMjudge（换为本程序的配色），支持 ICPC、OI、IOI 三种赛制，含答疑、封榜与滚榜、气球、打印。
- **界面**：中文 / English，跟随系统深浅色；同一套功能也可用命令行。

## 快速开始

1. 准备比赛目录（与 LemonLime 相同）：

   ```
   比赛目录/
     data/<试题>/        测试数据（*.in 与 *.out / *.ans）、校验器 checker.cpp、交互库……
     source/<选手>/      选手文件，可直接放，也可放在 <试题>/ 子文件夹中
   ```

2. 运行 `OIJudger.exe`，点击“打开比赛文件夹”或把文件夹拖进窗口。首次打开会自动识别试题并生成 `contest.toml`；若目录中有 LemonLime 的 `.cdf`，则自动导入其配置。
3. 点击“评测未测试/已变化”。

编译器：自动查找 PATH 与常见目录中的 MinGW（`g++`/`gcc`）；便携版自带编译器（`mingw64/` 文件夹），解压即可评测。也可在“设置”中手动指定。

建议在“设置 → 沙箱自检”中运行一次自检，确认本机上隔离与限制全部生效。

## 命令行

`oijudger-cli.exe`（`OIJudger.exe` 带参数运行时也是命令行模式）：

```
oijudger-cli judge <比赛目录> [-p 试题]... [-c 选手]... [--all] [-j 线程数] [--two-phase] [--cache-limit MB]
oijudger-cli results <比赛目录> [--contestant 选手]
oijudger-cli stats <比赛目录>
oijudger-cli export <比赛目录> -f html|htm|xlsx|csv|cdf [-o 文件]   # cdf：LemonLime 比赛文件（含成绩）
oijudger-cli similarity <比赛目录> [-p 试题] [-t 阈值%]
oijudger-cli clean <比赛目录>              # 删除编译缓存，评测结果保留
oijudger-cli init <比赛目录>
oijudger-cli import-lemon <比赛.cdf>
oijudger-cli compilers
oijudger-cli selftest
oijudger-cli serve <比赛目录> [--bind 0.0.0.0:8080]      # 在线比赛
oijudger-cli accounts import <比赛目录> <名单.csv> [-o 密码.csv]
oijudger-cli accounts list <比赛目录>
oijudger-cli accounts reset <比赛目录> <用户名>
```

## 配置文件 `contest.toml`

```toml
title = "模拟赛"

[[problem]]
name = "sum"            # 试题名，也是默认的源文件名与数据目录
title = "A + B"
stdio = true            # 标准输入输出；否则读写 input_file / output_file（默认 sum.in / sum.out）
comparator = "lines"    # lines（默认，忽略行末空格）| strict | tokens | ignore_spaces | real | binary
# checker = { path = "sum/checker.cpp", kind = "testlib" }   # 或 kind = "lemon"
# compile_flags = { cpp = "-O2 -std=c++14 -static -lm" }    # 本题的编译参数（替换“设置”中的）

[[problem.subtask]]     # 即 LemonLime 的“测试点”，得分取其中各组数据的最低分
score = 40
time_ms = 1000
memory_mb = 512
cases = [{ input = "sum/1.in", output = "sum/1.out" }]

[[problem.subtask]]
score = 60
time_ms = 1000
memory_mb = 512
depends = [1]           # 依赖的测试点；依赖得 0 分则跳过
cases = [{ input = "sum/2.in", output = "sum/2.out" }, { input = "sum/3.in", output = "sum/3.out" }]
```

交互题：`kind = "interactive"`，并配置 `grader = { header_path, header_name, grader_path }`（NOI 风格）或 `interactor = "guess/interactor.cpp"`（testlib 交互器）。提交答案题：`kind = "answer"`，`answer_ext = "out"`。通信题：`kind = "communication"`，`communication = { sources = [...], interfaces = [...] }`；加上 `multi_program = true` 则每个源文件编译为独立程序，`interfaces` 中的 `grader.cpp` 单独编译运行并负责启动选手程序（与 LemonLime 的多程序通信题相同；grader 与选手程序在同一个沙箱内，进程数上限为程序数 + 1）。

## 与 LemonLime 的兼容

- **导入**：打开含 `.cdf` 的比赛文件夹（还没有 `contest.toml` 时），或运行 `oijudger-cli import-lemon <比赛.cdf>`。
  - 比较方式“忽略多余空格”（LemonLime 默认）导入为 `ignore_spaces`：按单词比较，且单词须在同一行，与 LemonLime 判法相同。
  - 每题的编译配置（如 `CCF`）从本机 LemonLime 的设置中读出实际参数，写入 `compile_flags`；本机没有该配置时给出提示，并使用 OIJudger 的编译设置。
  - `.cdf` 里 LemonLime 已有的成绩不导入，由 OIJudger 重新评测。
- **导出**：“比赛 → 导出成绩 → LemonLime 比赛（.cdf）”或 `oijudger-cli export <比赛目录> -f cdf`，生成带 OIJudger 成绩的 `.cdf`，可用 LemonLime 打开查看、导出。
  - 默认保存为比赛目录下的 `<标题>-OIJudger.cdf`，不覆盖原来的 `.cdf`（LemonLime 按 `.cdf` 所在目录找 `data/`、`source/`，所以要放在比赛目录里）。
  - 试题与原 `.cdf` 一致时，原样保留其试题设置；否则按 `contest.toml` 转换，LemonLime 没有的设置会给出提示。
  - LemonLime 每个测试点只记整数分，小数分会四舍五入（会提示）。
- **超时复测**（默认开启，可在“设置”中关闭，命令行 `--no-recheck`）：
  - OIJudger 同时评测多个程序，吃内存的程序会互相抢内存带宽而变慢；LemonLime 一次只评一个，没有这个问题。
  - 所以全部评完后，把超时的测试点在没有其他评测时逐个单独重测；某题重测后仍超时，就不再重测该题其余超时的点。
  - 用 LemonLime 实测过的 47 名选手对比：并行评测时 1 道题因此被误判超时，单独重测后与 LemonLime 相同。

## 在线比赛（局域网）

评测机开启网页服务，选手用浏览器访问 `http://评测机IP:8080/`。页面结构与操作方式同 DOMjudge，只是换成了本程序的配色。

**启动**
- 桌面程序：打开比赛后，选“比赛 → 在线比赛（局域网）…”，点击“启动服务”。
- 命令行：`oijudger-cli serve <比赛目录>`。
- 首次启动会创建管理员账号 `admin`，并显示一次密码。忘记密码可用 `oijudger-cli accounts reset <比赛目录> admin` 重置。
- 首次启动时 Windows 防火墙会询问是否允许访问网络，请允许“专用网络”。

**页面**
| 页面 | 地址 | 使用者 |
|---|---|---|
| 选手首页 | `/team` | 选手：本队成绩行、提交记录、答疑与提问；顶栏绿色“提交”按钮 |
| 题目集 | `/team/problems` | 选手：题面、样例、时空限制、各时段通过情况 |
| 打印 | `/team/print` | 选手：提交打印任务 |
| 榜单 | `/scoreboard` | 所有人；`?projector=1` 为投影模式（自动滚动） |
| 评委 | `/jury` | 评委 / 管理员：提交、重测、忽略、答疑、账号、比赛设置 |
| 滚榜 | `/jury/resolver` | 评委：ICPC 赛后逐个揭晓封榜期间的结果 |
| 气球 / 打印队列 | `/balloons`、`/print-queue` | 志愿者 |

**账号**：管理员在“账号”页导入 CSV（`用户名,显示名称,学校,座位[,密码]`），或运行 `oijudger-cli accounts import`。生成的密码只显示一次，可导出 CSV 或打印密码条。角色有选手、评委、志愿者、管理员。

**赛制**
- **ICPC**：实时显示结果；按通过题数和罚时排名（每次错误提交罚时 20 分钟，编译错误默认不计）；遇到第一个未通过的测试点即停止评测；比赛结束前可封榜；可设金银铜牌数。
- **IOI**：实时显示各子任务得分；每题取各子任务最高分之和（也可设为取单次提交最高分）。
- **OI**：比赛中只显示“已提交”（也可设为告知是否编译通过）；赛后评委点“赛后评测”，用每人每题最后一次提交评测，再“公布成绩”。

**与 DOMjudge 相同的规则**
- **提交**：
  - 选择源文件后，按文件名自动识别题目（题号字母或题目名）和语言，确认后提交。
  - 比赛结束后的提交记为“太迟”（TOO-LATE），保存但不评测。
- **封榜**：其他人的榜单停止更新；选手首页上本队那一行照常更新，名次显示为“?”。
- **重测与忽略**：
  - 评委可重测单个提交，或某题 / 某队的全部提交。已正确的提交只有管理员勾选后才重测。
  - 被忽略的提交如同没有提交过。
- **公平排队**：已有提交在排队或评测中的队伍，新提交优先级较低。

**配置**：`contest.toml` 的 `[online]` 段，也可在评委页面“比赛设置”中修改。

```toml
[online]
format = "icpc"              # icpc | oi | ioi
start = "2026-11-08 09:00"   # 本地时间
duration_min = 300
freeze_min = 60              # 结束前多少分钟封榜，0 不封榜
penalty_min = 20
medals = [4, 4, 4]           # 金 / 银 / 铜牌数
submit_interval_s = 10       # 两次提交最短间隔
bind = "0.0.0.0:8080"

[[problem]]
name = "sum"
letter = "A"                 # 题号
color = "#e53935"            # 气球颜色
statement = "sum/statement.pdf"   # PDF、Markdown（支持 $公式$）或 HTML，放在 data/ 下
samples = ["sum/sample1.in", "sum/sample1.ans"]   # 省略时自动使用 data/<题>/sample*
```

**数据与离线工具**：
- 每次提交都存档在 `.oij/submissions/`。
- ICPC / OI 的最后一次提交、IOI 的最高分提交会写入 `source/<选手>/<题>/`，所以桌面程序的离线评测、统计、查重、导出都能直接使用。

**安全**：
- 评测仍在沙箱中进行。
- 选手只能看到自己的提交，比赛开始前看不到题目，测试数据不会通过网页提供。
- 会话 Cookie 为 HttpOnly + SameSite=Strict，写操作有 CSRF 校验，登录失败有频率限制。
- 网页服务为 HTTP，适合局域网 / 机房，不建议直接暴露到公网。

## 编译缓存占用空间

编译好的程序缓存在比赛目录的 `.oij/build/` 中，这样重新评测时不必再次编译。为了与正式评测环境一致，程序是静态链接的；OIJudger 已去掉其中的调试符号，每个程序约 0.25～1 MB，2 万份代码约需 5～20 GB。

评测时，缓存有大小上限（默认 10 GB，在“设置”中修改，0 表示不限；命令行用 `--cache-limit <MB>`）。超过上限时编译会暂停，已评测完的程序会先从缓存中删除，腾出空间后自动继续；等待评测或正在运行的程序不会被删除。上限只在“编译与评测同时进行”时生效。被删掉的程序以后如需重新评测，会重新编译。

比赛评完、不再需要重新评测时，用“工具 → 清理编译缓存”（或 `oijudger-cli clean <比赛目录>`）删除缓存。评测结果会保留，之后如需重新评测，程序会重新编译。

## 打不开 / 一打开就退出？

如果程序放在带“低完整性”标签的文件夹里（一些沙箱或安全工具管理的目录会这样设置），Windows 会以受限权限运行它：界面无法创建，评测沙箱也无法使用。OIJudger 会弹窗说明这一情况。把压缩包重新解压到普通文件夹（例如 `D:\OIJudger` 或桌面）即可。

## 关于 Windows Defender

实时防护会扫描每个新生成的可执行文件（每个约 0.1～1 秒，大量编译时会排队）。OIJudger 已把每个程序的扫描减到一次，但仍是编译耗时的主要部分之一。如需更快，可以（以管理员身份）把评测临时目录加入排除项：

```
Add-MpPreference -ExclusionPath "$env:LOCALAPPDATA\OIJudger"
Add-MpPreference -ExclusionPath "D:\比赛目录\.oij"   # 编译缓存，程序从这里运行
```

这只影响速度，不影响沙箱的安全性。

## 从源码构建

参与开发请先读 [交接文档](docs/HANDOVER.md)：代码结构、测试、发布流程与关键设计。

需要 Rust 与 Node.js。推荐 MSVC 工具链（Visual Studio Build Tools，含“使用 C++ 的桌面开发”）：发布版即用它构建，`.cargo/config.toml` 让 C 运行库静态链接，用户无需安装 VC++ 运行库。

```
cd app/ui && npm install && npm run build
cd app/web && npm install && npm run build      # 在线比赛网页（编译时嵌入服务端）
cargo build --release -p oijudger --features oijudger/custom-protocol
cargo build --release -p oij
cargo test --workspace
```

也可以用 GNU 工具链构建：项目路径含空格时，请设置不含空格的 `CARGO_TARGET_DIR`（MinGW 的 `windres` 不支持空格路径）。GNU 构建的 `OIJudger.exe` 需要同目录下的 `WebView2Loader.dll`（构建时会生成在 `release/` 中，`scripts/package.py` 会一并打包）；MSVC 构建则不需要。

打包：`python scripts/package.py <CARGO_TARGET_DIR> [MinGW 目录]`，生成 `dist/` 下的普通版与便携版压缩包。

## 许可

MIT。`assets/testlib/testlib.h` 来自 [testlib](https://github.com/MikeMirzayanov/testlib)（MIT）。
