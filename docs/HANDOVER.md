# OIJudger 交接文档

本文写给接手开发的人：项目现在是什么状态、代码在哪、怎么构建、测试、发布，以及最容易踩的坑。使用说明见 [README](../README.md)。

## 1. 现状

- 仓库：<https://github.com/qiuyu07215/OIJudger>（公开，MIT 许可），主分支 `master`。
- 最新版本：**v0.1.1**（Releases 页提供普通版与带编译器的便携版两个压缩包）。
  - v0.1.0 的标签指向的提交与发布文件不完全对应：发布文件后来换成了 MSVC 构建，但标签没有移动。v0.1.1 的标签与发布文件一致。
- 平台：只支持 Windows 10/11 x64。沙箱接口已为以后的 Linux 后端预留，但尚未实现。
- 功能已全部实现：
  - 四种题型：传统、提交答案、交互、通信（含多程序通信）。
  - 沙箱、增量评测、LemonLime `.cdf` 导入。
  - 统计、查重、导出（HTML / Excel / CSV）。
  - 中英文界面、命令行。
- 代码量：Rust 约 9k 行，前端约 3.5k 行。

## 2. 目录结构

```
Cargo.toml              workspace（版本号在这里）
.cargo/config.toml      MSVC 下静态链接 C 运行库
crates/
  sandbox/              Windows 沙箱：AppContainer + Job Object、ACL、CPU 拓扑、PE 头
  judge-core/           比赛模型、发现选手/数据、编译与缓存、评测、比较器、校验器、调度
  store/                SQLite 结果库（.oij/results.db）
  analysis/             统计指标、代码查重（winnowing）
  export/               CSV / XLSX / HTML 导出
  lemon-import/         LemonLime .cdf（JSON）→ contest.toml
  cli/                  命令行（crate 名 oij，可执行文件 oijudger-cli），自检、端到端测试
app/
  src-tauri/            Tauri v2 主程序 OIJudger.exe；带参数启动时走命令行模式
  ui/                   Vue 3 + TypeScript + Naive UI + ECharts + vue-i18n
assets/testlib/         testlib.h（编译进程序，评测时写入 .oij/testlib.h）
tests/sandbox-escape/   沙箱逃逸测试用的 C++ 程序
examples/demo-contest/  演示比赛 + expected.json（由 scripts/make_demo.py 生成）
scripts/                make_demo.py、make_bench.py、package.py、make_icon.py
```

### judge-core 各文件

| 文件 | 内容 |
|---|---|
| `model.rs` | `contest.toml` 的数据结构（Contest / Problem / Subtask / Case），`ContestDir` 路径约定 |
| `discover.rs` | 列选手、找源文件（子文件夹 auto/inside/outside）、自动识别试题与测试点配对 |
| `compile.rs` | 编译器探测、预编译头（PCH）、`build()`：沙箱内编译 + 按内容哈希缓存 |
| `judge.rs` | 评测单个（选手, 试题）：运行、比较、校验器、交互、多程序通信、计分 |
| `engine.rs` | 并行调度：编译线程 + 评测线程、编译与评测同时进行、进度事件 |
| `workspace.rs` | 每个线程的沙箱身份与临时目录、核心绑定、崩溃残留清理 |
| `checker.rs` | testlib / Lemon 校验器的运行与结果解析 |
| `compare.rs` | 内置比较器（逐行、严格、按词、实数、二进制，内存映射） |
| `result.rs` | Verdict、各级结果结构 |
| `config.rs` | 全局设置 `%APPDATA%\OIJudger\settings.toml` |

### 前端

- `Shell.vue`：菜单栏、工具栏、标签页（概要 / 结果 / 统计 / 查重）、状态栏。
- `views/`：各页面。
- `components/JudgingDialog.vue`：评测进度窗口。
- `api.ts`：调用后端 Tauri 命令（后端在 `app/src-tauri/src/commands.rs`）。
- `mock.ts`：浏览器预览用的假后端，仅开发时生效。
- 配色在 `theme.ts` 与 `style.css`。

## 3. 开发环境与构建

需要：

- Rust stable（推荐 `x86_64-pc-windows-msvc`）。
- Visual Studio Build Tools 2022（“使用 C++ 的桌面开发”，含 Windows SDK）。
- Node.js。
- MinGW-w64 g++：评测时用来编译选手程序，测试也要用。本机在 `C:\mingw64`。

```
cd app/ui && npm install && npm run build      # 前端 → app/ui/dist
cargo build --release -p oijudger -p oij --features oijudger/custom-protocol
```

- 必须带 `--features oijudger/custom-protocol`：没有它，`OIJudger.exe` 会去连开发服务器 `localhost:5173`。
- MSVC 构建不依赖 VC++ 运行库：CLI 完全静态；GUI 只用 Windows 自带的 UCRT，`.cargo/config.toml` 负责这一点。
- 也能用 GNU 工具链（`cargo +stable-x86_64-pc-windows-gnu`），但要注意两点：
  - 项目路径有空格时，MinGW 的 `windres` 会失败，需设置不含空格的 `CARGO_TARGET_DIR`。
  - GNU 构建的 `OIJudger.exe` 运行时需要同目录的 `WebView2Loader.dll`。
- 只改前端时，用 `.claude/launch.json` 里的 `ui-dev`（`npm --prefix app/ui run dev`，端口 5173）在浏览器预览，数据来自 `mock.ts`。
- 用真实后端调试：`cd app/src-tauri && npx tauri dev`。

## 4. 测试与验证

| 做什么 | 命令 | 备注 |
|---|---|---|
| 全部单元 / 集成测试 | `cargo test --release --workspace` | 沙箱测试要用 g++ 编译探针程序：PATH 里要有 g++，或设 `OIJ_GXX` |
| 演示比赛端到端 | `cargo test --release -p oij --test demo` | 与 `examples/demo-contest/expected.json` 逐项对比 |
| 沙箱自检（16 项） | `oijudger-cli selftest` | 断网、文件隔离、子进程、剪贴板、TLE/MLE/OLE、深递归、编译期 `#include` |
| 性能基准 | `python scripts/make_bench.py <目录> 120 10`，然后 `oijudger-cli judge <目录>` | 每个选手的源码都不同，编译缓存无法复用 |

调试用环境变量：

- `OIJ_DEBUG=1`：打印每次编译与运行的耗时、CPU 时间和沙箱准备时间。
- `OIJ_BUILDERS=N`：覆盖编译线程数。

基准数据（i5-12400，6 核 12 线程，120 人 × 4 题 × 10 点，Defender 开启）：

| 场景 | 耗时 |
|---|---|
| 从零开始完整评测 | 约 180 秒 |
| 已有编译缓存，全部重评 | 约 64 秒 |
| 无改动，再次评测 | 0.2 秒 |

修改 `scripts/make_demo.py` 后要重新生成演示比赛，并同步 `expected.json` 与 `crates/cli/tests/demo.rs` 里的任务数。

## 5. 发布流程

1. 改版本号（四处）：`Cargo.toml`、`app/src-tauri/tauri.conf.json`、`app/ui/package.json`、`app/ui/package-lock.json`（两处）。
2. `npm run build`（app/ui），然后用 MSVC 执行上面的 `cargo build --release ...`。
3. 运行测试，并跑一次 `oijudger-cli selftest`。
4. `python scripts/package.py <CARGO_TARGET_DIR> [MinGW 目录，默认 C:\mingw64]`，在 `dist/` 生成两个压缩包：
   - 便携版带上整个 MinGW（`opt/` 除外），其许可证在 `mingw64/licenses/`。
   - 如果 `dist/` 下的程序还开着，打包会因文件被占用而失败。
5. 解压到新目录验证：
   - GUI 能打开窗口。
   - 在只有 `C:\Windows\System32` 的 PATH 下，便携版能识别自带编译器、自检通过、演示比赛分数正确。
6. 提交并打标签，然后发布：
   ```
   git tag vX.Y.Z && git push origin master vX.Y.Z
   gh release create vX.Y.Z dist/*.zip --title ... --notes-file ...
   ```

程序没有代码签名，首次运行时 Windows SmartScreen 可能提示“未知发布者”。

## 6. 关键设计与踩过的坑

### 沙箱（crates/sandbox）

**基本隔离**
- 选手程序和编译器都运行在**不带任何能力**的 AppContainer 里：内核层面断网（含本机回环与 DNS），只能访问被显式授权的目录。
- Job Object 负责：
  - 内存限制、进程数限制、UI 限制。
  - 关闭 Job 即杀死全部进程；未处理异常直接结束，不弹错误窗口。
  - CPU 时间硬上限。
- 继承句柄用 `PROC_THREAD_ATTRIBUTE_HANDLE_LIST` 限定，只传标准输入输出。

**沙箱身份**
- 每个线程有两个独立身份：运行用 `OIJudger.<pid>.W<i>`，编译用 `OIJudger.<pid>.B<i>`。
- 名字带进程号，两个同时运行的实例互不共享。
- 退出时删除这些身份；进程崩溃留下的残留，由下次启动根据 `.lock` 文件清理。

**沙箱自己的配置文件夹**
- 位置是 `%LOCALAPPDATA%\Packages\<名字>\AC`，默认可写，会造成跨运行的信息泄露，所以运行用的身份会锁住它。
- GCC 必须往 TEMP 写临时文件（Windows 下忽略 `TMPDIR`），所以编译用的是另一个身份：TEMP 可写，每次编译前清空。

**进程启动**
- 环境块里必须有 `USERPROFILE`、`LOCALAPPDATA`、`APPDATA`，否则 `CreateProcess` 报错 203。
- 单进程运行用 `DETACHED_PROCESS`，不创建 conhost，每次运行能从 26 ms 降到 9 ms。需要子进程时（编译器、多程序通信）用 `CREATE_NO_WINDOW`。

**计时与栈**
- CPU 时间用 `QueryProcessCycleTime` 除以 TSC 频率，精度到微秒；Job 记账只有 15.6 ms 的粒度。多进程运行时用 Job 记账。
- 栈大小：编译时加 `-Wl,--stack,<内存限制>`，否则 Windows 默认只有 2 MB 栈。

**编译器访问**
- 编译器目录和 PCH 目录授予 `ALL APPLICATION PACKAGES` 只读。如果编译器装在当前用户无权改 ACL 的目录，会提示改用便携版。

**可信程序**
- 校验器和交互器是出题人提供的可信程序：不进 AppContainer（`Container::unisolated()`），但仍受 Job 资源限制。

### 编译与 Windows Defender（性能关键）

- 每个**新写入的 exe** 首次运行前都会被 Defender 扫描（约 0.1～1 秒）。大量编译时扫描会排队，是编译慢的主要原因之一。
- 现在每个程序只被扫描一次：
  1. 链接器的输出直接**移动**（不是复制）进比赛目录的 `.oij/build/<哈希>.exe`。
  2. 评测时**直接从缓存运行**：临时给当前线程的沙箱身份授予这一个文件的读取权限，用完收回；修改 ACL 有全局锁串行化。
  3. 不再复制到运行目录，也不再事后修改 PE 头。
- **坑：低完整性标签。** 沙箱里的链接器生成的文件带有 Low 完整性标签。直接移动会保留这个标签，Windows 从带 Low 标签的文件启动程序时会降为 Low 完整性，结果可信的校验器写不出结果。所以入缓存时要用 `sandbox::reset_acl` 重置 ACL 并去掉标签。
- 预编译 `bits/stdc++.h`：
  - 按“编译器 + 版本 + 参数”在 `%LOCALAPPDATA%\OIJudger\pch\<key>\` 生成一次，通过 `-I` 让 GCC 优先找到 `.gch`。
  - 参数不匹配时，GCC 会静默改用原头文件。
- 编译时总是加 `-s` 去掉符号表：静态链接的程序约小 60%（scanf 程序 575 KB → 248 KB，iostream 程序 2.5 MB → 1.0 MB），运行行为不变。缓存键里有 `strip` 标记，与旧缓存区分开。
- **缓存上限**（`Settings::cache_limit_mb`，默认 10 GB，仅在编译与评测同时进行时生效）：
  - 正在使用的程序由 `compile::Lease` 保护：编译完成或命中缓存时取得租约。等待评测的任务带着租约排在 `ready` 队列里，评测时还会再持有一份，评测完才释放。查缓存和取租约在同一把锁下进行。
  - 编译前 `make_room` 检查估算的占用。超过上限时，`compile::evict` 删除没有租约的程序，直到占用降到上限的 90%：先删本次已评测完的，再按修改时间从旧到新删。
  - 仍然不够时编译暂停（`Event::CompilePaused`，界面显示“编译已暂停”），等有任务评测完再试。
  - 如果没有任何任务在编译、排队或评测，就照常继续，避免卡死。
  - 校验器 / 交互器的租约永不释放（`keep_forever`）。评测结束时再修剪一次，使缓存回到上限以内。
  - 实测：1 MB 上限下暂停 / 恢复 24 次，没有卡死，分数一致；整次评测花在腾空间上的时间不到 1 秒。
- 也可以手动清空缓存：GUI“工具 → 清理编译缓存”或 `oijudger-cli clean`（`compile::clear_cache`）。刚评测完时，个别 exe 可能被短暂占用，删除时会等几秒再重试。
- 编译缓存的键是 blake3（源码 + 附加文件 + 编译器版本 + 参数）。编译失败也会缓存。同一个键同时只有一个线程在编译，其他线程等待（`Claim`）。

### 调度（engine.rs）

- **默认：编译与评测同时进行。**
  - 评测线程绑定到不同的性能核（默认 = 混合架构的 P 核数，否则物理核数的 3/4）。
  - 编译线程只用剩下的核心，`spare_cpu_mask` 会排除评测核心的超线程兄弟。
  - 评测线程没有可评的任务时，自己编译下一份。
  - 共享状态是 `Schedule`（Mutex + Condvar）。
- 实测编译与评测同时进行时，各题平均运行时间与先编译后评测相比在波动范围内。
- `--two-phase` 或设置里的开关可恢复为：先用全部逻辑核编译，再评测。

### 判分语义（与 LemonLime 一致）

- `contest.toml` 里的 `subtask` 即 LemonLime 的“测试点”：
  - 得分 = 其中各组数据得分比例的最低值 × 分值。
  - 可设依赖；依赖得 0 分则跳过。某组数据得 0 分后，同测试点的剩余数据跳过。
- **testlib 校验器**：
  - 先解析 stderr 里的 `points X` 或 `partially correct (N)`，再看退出码。因为 `PC_BASE_EXIT_CODE` 为 0，`_pc` 的退出码会和其他结果撞。
  - Lemon 校验器使用 6 参数协议。
- **testlib 交互器**：两个进程用管道相连。谁先结束以谁的结果为准（`interactor_first`），避免选手后来的 TLE 掩盖交互器已判出的 WA。
- **增量评测**：每条结果记录源码指纹和试题指纹（配置、数据文件大小与修改时间、编译器版本与参数、相关设置），任一变化即视为过期。

### LemonLime 导入（lemon-import）

- `.cdf` 是 JSON，键名是 LemonLime 的 C++ 成员名。
- 依赖关系编码在形如 `"<n>_lemon_SUbtaskDEPENDENCE_fLAg"` 的键里。
- `taskType` 0～4，其中 4 = 多程序通信；`comparisonMode` 0～5。

### 其他

- **低完整性文件夹**：
  - 从带 Low 完整性标签的文件夹启动的程序会以 Low 级别运行。这时 WebView2 报 `0x800700AA`，Tauri 在启动阶段 panic 退出，GUI 没有控制台，用户只看到“闪退”。
  - 现在启动时会检查 `sandbox::integrity_level()`：GUI 弹窗说明，CLI 报错退出。
  - GUI 另外装了 panic hook，任何启动错误都会弹窗显示，不再静默退出。
- 文件操作偶尔会被 Defender 或刚退出的进程短暂占用，所以用 `retry_io` 重试；旧临时目录在后台线程删除。
- 统一使用 LF 换行（`.gitattributes`）。用脚本批量改文件时注意不要写入 CRLF。

## 7. 运行时数据位置

| 位置 | 内容 |
|---|---|
| `%APPDATA%\OIJudger\settings.toml` | 全局设置（编译器、线程数、语言、最近打开） |
| `%LOCALAPPDATA%\OIJudger\work\<pid>\` | 各线程的编译 / 运行临时目录，退出时删除 |
| `%LOCALAPPDATA%\OIJudger\pch\` | 预编译头 |
| `<比赛目录>\.oij\results.db` | 评测结果（SQLite，WAL，每个（选手, 试题）一行，详情为 JSON） |
| `<比赛目录>\.oij\build\` | 编译缓存（exe、编译日志、失败记录） |

## 8. 已知限制与可做的事

- **Linux 后端**：未实现。计划用 namespace + seccomp + cgroup 实现同一套沙箱接口。
- **超大规模**：没在 2 万份代码级别实测过。按基准推算，在 6 核机器上约需两小时，瓶颈是 gcc 本身和每个程序一次的 Defender 扫描。可做的方向：
  - 把评测临时目录和比赛的 `.oij` 加入 Defender 排除项（需管理员，已写在 README）。
  - 多机分布式评测（尚未设计）。
- **GUI 没有自动化测试**：界面改动目前靠手动和浏览器预览（mock 后端）检查。
- **路径可见**：选手程序从缓存运行，能通过 `GetModuleFileName` 看到比赛目录的路径，但读不到其中任何文件（已测试）。
- **前端包偏大**：主 JS 约 1.5 MB（ECharts 和 Naive UI），对本地桌面程序影响不大；需要时可按页面拆分。
- **没有代码签名**：首次运行会有 SmartScreen 提示。
