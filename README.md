# OIJudger

安全、高效的 OI 本地评测工具（Windows）。目录结构与 LemonLime 兼容，可直接导入 LemonLime 的 `.cdf` 比赛。

- **沙箱隔离**：选手程序运行在无任何能力的 AppContainer 中——内核层面断网（含本机回环与 DNS），只能读写自己的临时目录，不能读取标准答案、其他选手文件或用户文件，不能创建子进程、读剪贴板；编译过程同样在沙箱内（`#include` 外部文件会失败）。无需管理员权限。
- **资源限制**：内存由 Job Object 在内核强制；按 CPU 时间判超时，墙钟兜底防止 `sleep`/阻塞；输出大小限制；栈空间自动设为内存限制（Windows 默认 2 MB 栈会让深递归爆栈）。
- **高效**：多线程并行评测（默认绑定到性能核，计时稳定）；编译与评测同时进行（编译器只使用评测线程之外的核心，不影响计时）；预编译 `bits/stdc++.h`；编译结果缓存，程序直接从缓存运行，避免杀毒软件重复扫描；增量评测（只评测源程序、数据或设置发生变化的部分）。
- **题型**：传统题（文件 / 标准 IO）、提交答案题、交互题（NOI 风格 grader，或 testlib 交互器双进程）、通信题（多源文件合编，或多程序：由 grader 启动各选手程序）。
- **判分**：多种比较方式、testlib 校验器、Lemon 6 参数校验器、测试点内多组数据取最低分、子任务依赖。
- **分析**：分数分布（含前缀和/后缀和）、平均分、中位数、标准差、区分度、测试点通过率、试题相关性、按 `contestants.csv` 分组对比；代码查重（忽略变量名、注释与格式）。
- **导出**：HTML（含/不含详细结果）、Excel、CSV。
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
oijudger-cli judge <比赛目录> [-p 试题]... [-c 选手]... [--all] [-j 线程数] [--two-phase]
oijudger-cli results <比赛目录> [--contestant 选手]
oijudger-cli stats <比赛目录>
oijudger-cli export <比赛目录> -f html|htm|xlsx|csv [-o 文件]
oijudger-cli similarity <比赛目录> [-p 试题] [-t 阈值%]
oijudger-cli init <比赛目录>
oijudger-cli import-lemon <比赛.cdf>
oijudger-cli compilers
oijudger-cli selftest
```

## 配置文件 `contest.toml`

```toml
title = "模拟赛"

[[problem]]
name = "sum"            # 试题名，也是默认的源文件名与数据目录
title = "A + B"
stdio = true            # 标准输入输出；否则读写 input_file / output_file（默认 sum.in / sum.out）
comparator = "lines"    # lines（默认，忽略行末空格）| strict | tokens | real | binary
# checker = { path = "sum/checker.cpp", kind = "testlib" }   # 或 kind = "lemon"

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
cargo build --release -p oijudger --features oijudger/custom-protocol
cargo build --release -p oij
cargo test --workspace
```

也可以用 GNU 工具链构建：项目路径含空格时，请设置不含空格的 `CARGO_TARGET_DIR`（MinGW 的 `windres` 不支持空格路径）。GNU 构建的 `OIJudger.exe` 需要同目录下的 `WebView2Loader.dll`（构建时会生成在 `release/` 中，`scripts/package.py` 会一并打包）；MSVC 构建则不需要。

打包：`python scripts/package.py <CARGO_TARGET_DIR> [MinGW 目录]`，生成 `dist/` 下的普通版与便携版压缩包。

## 许可

MIT。`assets/testlib/testlib.h` 来自 [testlib](https://github.com/MikeMirzayanov/testlib)（MIT）。
