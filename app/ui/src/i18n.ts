import { createI18n } from "vue-i18n";

const zh = {
  app: { name: "OIJudger", tagline: "安全、高效的 OI 本地评测工具" },
  common: {
    save: "保存", cancel: "取消", revert: "撤销修改", close: "关闭", delete: "删除", add: "添加",
    edit: "编辑", up: "上移", down: "下移", apply: "应用", ok: "确定", yes: "是", no: "否",
    loading: "加载中…", unsaved: "有未保存的修改", saved: "已保存", none: "无", auto: "自动",
    search: "搜索", refresh: "刷新", openFolder: "打开文件夹", ms: "毫秒", mb: "MiB",
  },
  welcome: {
    open: "打开比赛文件夹",
    hint: "把比赛文件夹拖到这里，或点击上方按钮选择",
    recent: "最近打开",
    layoutTitle: "目录结构（与 LemonLime 相同）",
    layout:
      "比赛目录/\n  data/<试题>/        测试数据、校验器、交互库\n  source/<选手>/      选手文件，可放在 <试题>/ 子文件夹内\n  contest.toml        试题配置（首次打开时自动生成）",
    noData: "没有 data 目录时会自动创建空目录，之后可在“试题”页添加试题。",
  },
  nav: { results: "成绩", problems: "试题", stats: "统计", plagiarism: "查重" },
  toolbar: {
    judgeStale: "评测未测试/已变化", judgeAll: "全部重测", judgeSelected: "评测选中",
    stop: "停止", reload: "刷新", openDir: "打开比赛目录", settings: "设置", closeContest: "关闭比赛",
    staleCount: "{n} 项待评测",
  },
  results: {
    rank: "名次", contestant: "选手", total: "总分", time: "用时", filter: "筛选选手…",
    contestants: "{n} 名选手", problems: "{n} 道试题", noContestants: "source 目录中还没有选手文件夹",
    stale: "源程序、数据或设置已变化，需要重新评测", notJudged: "未评测",
  },
  detail: {
    cases: "测试点", compile: "编译信息", source: "源代码", rejudge: "重新评测", subtask: "子任务",
    input: "输入文件", verdict: "结果", score: "得分", memory: "内存", message: "信息",
    noResult: "尚未评测", noSource: "未找到源文件", judgedAt: "评测于 {t}", empty: "（无）",
  },
  status: {
    Judged: "已评测", NoSource: "缺少文件", CompileError: "编译错误", SystemError: "系统错误",
  },
  verdict: {
    AC: "正确", PC: "部分正确", WA: "答案错误", TLE: "超时", MLE: "超内存", OLE: "输出超限",
    RE: "运行错误", NoOutput: "无输出", Skipped: "跳过", SystemError: "系统错误",
    CE: "编译错误", Missing: "缺少文件", SE: "系统错误", NO: "无输出", SKIP: "跳过",
  },
  progress: {
    title: "评测中", done: "已完成 {done} / {total}", elapsed: "用时 {t}", eta: "预计剩余 {t}",
    idle: "空闲", finished: "评测完成", cancelled: "已停止", workers: "评测线程",
  },
  problems: {
    list: "试题列表", detect: "自动识别试题", detectHint: "从 data/ 子目录识别新的试题（不会覆盖已有试题）",
    add: "新建试题", none: "还没有试题。把数据放进 data/<试题名>/，然后点“自动识别试题”。",
    basics: "基本信息", name: "试题名（目录名）", title: "显示名称", kind: "题型", sourceName: "源文件名（不含扩展名）",
    subfolder: "源文件位置", io: "输入输出", stdio: "标准输入输出", inputFile: "输入文件名", outputFile: "输出文件名",
    judge: "判分方式", comparator: "比较方式", eps: "实数误差", checker: "自定义校验器（相对 data/）",
    checkerKind: "校验器类型", answerExt: "答案文件扩展名", grader: "交互库", headerPath: "头文件（相对 data/）",
    headerName: "选手包含的头文件名", graderPath: "grader 实现（相对 data/）", interactor: "testlib 交互器（相对 data/）",
    comm: "通信题文件", commSources: "选手源文件（每行一个）", commInterfaces: "接口文件（相对 data/，每行一个）",
    compileArgs: "附加编译参数", subtasks: "测试点", total: "总分 {n}", cases: "{n} 组数据",
    score: "分值", time: "时间 (ms)", memory: "空间 (MiB)", depends: "依赖", dataFiles: "数据文件",
    inputPath: "输入", outputPath: "答案", addCase: "添加数据", addSubtask: "添加测试点",
    bulk: "批量设置", splitScores: "平分 {n} 分", merge: "合并选中", split: "拆分选中",
    detected: "识别到 {n} 道新试题", noneDetected: "没有发现新的试题",
    removeConfirm: "删除试题 {name}？已有的评测结果会被保留。",
    pattern: "按模式添加", patternTitle: "按文件名模式添加数据",
    patternHelp: "在 data/ 中按正则匹配输入文件，例如 sword(\\d+)-(\\d+)\\.in 。第 1 个括号相同的数据归为同一测试点。",
    patternInput: "输入文件正则", patternOutput: "答案扩展名", patternDir: "目录（相对 data/）", preview: "预览",
    replace: "替换现有测试点",
  },
  kind: { traditional: "传统题", answer: "提交答案题", interactive: "交互题", communication: "通信题" },
  subfolder: { auto: "自动（优先子文件夹）", inside: "仅 <试题>/ 子文件夹", outside: "仅选手文件夹根目录" },
  comparator: {
    lines: "逐行比较（忽略行末空格，推荐）", strict: "逐行严格比较", tokens: "按单词比较",
    real: "实数比较", binary: "逐字节比较",
  },
  checkerKind: { testlib: "testlib", lemon: "Lemon（6 个参数）" },
  settings: {
    title: "设置", compilers: "编译器", detect: "自动检测", path: "路径", args: "编译参数", version: "版本",
    noCompiler: "没有找到编译器。请安装 MinGW，或使用自带编译器的便携版。", judging: "评测",
    workers: "并行线程数（0 = 自动，推荐 {n}）", pin: "将线程绑定到性能核（计时更稳定）",
    patchStack: "栈空间 = 内存限制（避免 Windows 默认 2MB 栈导致递归爆栈）", outputLimit: "输出上限 (MiB)",
    compileTime: "编译时间限制 (ms)", compileMemory: "编译内存限制 (MiB)", checkerTime: "校验器时间限制 (ms)",
    checkerMemory: "校验器内存限制 (MiB)", language: "界面语言", selftest: "沙箱自检",
    selftestHint: "在本机验证：断网、文件隔离、禁止子进程、资源限制等是否生效。",
    runSelftest: "运行自检", allPassed: "全部通过", someFailed: "有检查未通过", settingsFile: "配置文件：{p}",
  },
  msg: {
    openFailed: "打开失败：{e}", saveFailed: "保存失败：{e}", judgeFailed: "评测出错：{e}",
    nothingToJudge: "没有需要评测的项目", selectFirst: "请先在表格中勾选选手",
    dropFolder: "请拖入文件夹", savedProblems: "试题配置已保存",
  },
};

const en: typeof zh = {
  app: { name: "OIJudger", tagline: "Fast, sandboxed local judge for OI contests" },
  common: {
    save: "Save", cancel: "Cancel", revert: "Revert", close: "Close", delete: "Delete", add: "Add",
    edit: "Edit", up: "Up", down: "Down", apply: "Apply", ok: "OK", yes: "Yes", no: "No",
    loading: "Loading…", unsaved: "Unsaved changes", saved: "Saved", none: "None", auto: "Auto",
    search: "Search", refresh: "Refresh", openFolder: "Open folder", ms: "ms", mb: "MiB",
  },
  welcome: {
    open: "Open contest folder",
    hint: "Drop a contest folder here or use the button above",
    recent: "Recent",
    layoutTitle: "Folder layout (same as LemonLime)",
    layout:
      "contest/\n  data/<problem>/      test data, checkers, graders\n  source/<contestant>/ submissions, optionally in <problem>/ subfolders\n  contest.toml         problem configuration (created on first open)",
    noData: "Missing data/ and source/ folders are created; add problems on the Problems page.",
  },
  nav: { results: "Results", problems: "Problems", stats: "Statistics", plagiarism: "Similarity" },
  toolbar: {
    judgeStale: "Judge new / changed", judgeAll: "Rejudge all", judgeSelected: "Judge selected",
    stop: "Stop", reload: "Reload", openDir: "Open contest folder", settings: "Settings", closeContest: "Close contest",
    staleCount: "{n} to judge",
  },
  results: {
    rank: "Rank", contestant: "Contestant", total: "Total", time: "Time", filter: "Filter contestants…",
    contestants: "{n} contestants", problems: "{n} problems", noContestants: "No contestant folders in source/ yet",
    stale: "Source, data or settings changed — needs rejudging", notJudged: "Not judged",
  },
  detail: {
    cases: "Test cases", compile: "Compiler output", source: "Source", rejudge: "Rejudge", subtask: "Subtask",
    input: "Input", verdict: "Verdict", score: "Score", memory: "Memory", message: "Message",
    noResult: "Not judged yet", noSource: "Source file not found", judgedAt: "Judged at {t}", empty: "(empty)",
  },
  status: { Judged: "Judged", NoSource: "Missing", CompileError: "Compile error", SystemError: "System error" },
  verdict: {
    AC: "Accepted", PC: "Partially correct", WA: "Wrong answer", TLE: "Time limit", MLE: "Memory limit",
    OLE: "Output limit", RE: "Runtime error", NoOutput: "No output", Skipped: "Skipped", SystemError: "System error",
    CE: "Compile error", Missing: "Missing", SE: "System error", NO: "No output", SKIP: "Skipped",
  },
  progress: {
    title: "Judging", done: "{done} / {total} done", elapsed: "elapsed {t}", eta: "about {t} left",
    idle: "idle", finished: "Judging finished", cancelled: "Stopped", workers: "Workers",
  },
  problems: {
    list: "Problems", detect: "Detect problems", detectHint: "Detect new problems from data/ subfolders (existing ones are kept)",
    add: "New problem", none: "No problems yet. Put data into data/<problem>/ and click “Detect problems”.",
    basics: "Basics", name: "Name (data folder)", title: "Title", kind: "Type", sourceName: "Source name (no extension)",
    subfolder: "Source location", io: "Input / output", stdio: "Standard input / output", inputFile: "Input file",
    outputFile: "Output file", judge: "Scoring", comparator: "Comparison", eps: "Real number error",
    checker: "Custom checker (relative to data/)", checkerKind: "Checker protocol", answerExt: "Answer file extension",
    grader: "Interaction library", headerPath: "Header (relative to data/)", headerName: "Header name included by contestants",
    graderPath: "Grader (relative to data/)", interactor: "testlib interactor (relative to data/)",
    comm: "Communication files", commSources: "Contestant sources (one per line)",
    commInterfaces: "Interface files (relative to data/, one per line)", compileArgs: "Extra compiler arguments",
    subtasks: "Test points", total: "Total {n}", cases: "{n} files", score: "Score", time: "Time (ms)",
    memory: "Memory (MiB)", depends: "Depends on", dataFiles: "Data files", inputPath: "Input", outputPath: "Answer",
    addCase: "Add files", addSubtask: "Add test point", bulk: "Bulk edit", splitScores: "Split {n} points evenly",
    merge: "Merge selected", split: "Split selected", detected: "Detected {n} new problem(s)",
    noneDetected: "No new problems found", removeConfirm: "Delete problem {name}? Existing results are kept.",
    pattern: "Add by pattern", patternTitle: "Add data files by name pattern",
    patternHelp: "Regex for input files in data/, e.g. sword(\\d+)-(\\d+)\\.in . Files with the same first group form one test point.",
    patternInput: "Input regex", patternOutput: "Answer extension", patternDir: "Folder (relative to data/)",
    preview: "Preview", replace: "Replace existing test points",
  },
  kind: { traditional: "Traditional", answer: "Output only", interactive: "Interactive", communication: "Communication" },
  subfolder: { auto: "Auto (prefer subfolder)", inside: "Only <problem>/ subfolder", outside: "Only contestant root" },
  comparator: {
    lines: "Lines, ignore trailing spaces (recommended)", strict: "Lines, exact", tokens: "Tokens",
    real: "Real numbers", binary: "Bytes",
  },
  checkerKind: { testlib: "testlib", lemon: "Lemon (6 arguments)" },
  settings: {
    title: "Settings", compilers: "Compilers", detect: "Detect", path: "Path", args: "Arguments", version: "Version",
    noCompiler: "No compiler found. Install MinGW or use the portable bundle with a compiler.", judging: "Judging",
    workers: "Parallel workers (0 = auto, recommended {n})", pin: "Pin workers to performance cores (stabler timing)",
    patchStack: "Stack size = memory limit (avoids Windows' default 2 MiB stack)", outputLimit: "Output limit (MiB)",
    compileTime: "Compile time limit (ms)", compileMemory: "Compile memory limit (MiB)",
    checkerTime: "Checker time limit (ms)", checkerMemory: "Checker memory limit (MiB)", language: "Language",
    selftest: "Sandbox self test", selftestHint: "Verify on this machine that network, file isolation, child processes and limits are enforced.",
    runSelftest: "Run self test", allPassed: "All checks passed", someFailed: "Some checks failed",
    settingsFile: "Settings file: {p}",
  },
  msg: {
    openFailed: "Cannot open: {e}", saveFailed: "Cannot save: {e}", judgeFailed: "Judging failed: {e}",
    nothingToJudge: "Nothing to judge", selectFirst: "Select contestants in the table first",
    dropFolder: "Please drop a folder", savedProblems: "Problem configuration saved",
  },
};

export const i18n = createI18n({
  legacy: false,
  locale: "zh-CN",
  fallbackLocale: "en",
  messages: { "zh-CN": zh, en },
});

export function setLocale(l: string) {
  (i18n.global.locale as unknown as { value: string }).value = l === "en" ? "en" : "zh-CN";
  document.documentElement.lang = l === "en" ? "en" : "zh-CN";
}
