import { createI18n } from "vue-i18n";

// Pages follow DOMjudge's structure and wording (English) with Chinese
// equivalents.
const zh = {
  common: {
    ok: "确定", cancel: "取消", save: "保存", delete: "删除", edit: "编辑", add: "添加", close: "关闭",
    yes: "是", no: "否", loading: "加载中…", all: "全部", none: "无", actions: "操作", confirm: "确认",
    saved: "已保存", copy: "复制", copied: "已复制", download: "下载", back: "返回", search: "搜索", send: "发送",
    error: "出错了：{e}", filter: "筛选", apply: "应用", reset: "清除",
  },
  nav: {
    home: "首页", problemset: "题目集", print: "打印", scoreboard: "榜单", submit: "提交", logout: "退出",
    login: "登录", notifications: "开启通知", jury: "评委", submissions: "提交记录", clarifications: "答疑",
    balloons: "气球", printQueue: "打印队列", teams: "账号", contest: "比赛设置", resolver: "滚榜",
    projector: "投影模式",
  },
  clock: { before: "距开始 {t}", left: "剩余 {t}", over: "比赛已结束", notScheduled: "时间未定" },
  login: { title: "请登录", username: "用户名", password: "密码", submit: "登录", failed: "用户名或密码错误", tooMany: "失败次数过多，请一分钟后再试" },
  result: {
    correct: "正确", "wrong-answer": "答案错误", timelimit: "超时", "run-error": "运行错误", "compiler-error": "编译错误",
    "no-output": "无输出", "output-limit": "输出超限", "too-late": "太迟", pending: "等待中", submitted: "已提交",
    compiled: "编译通过", "judge-error": "评测出错",
  },
  submit: {
    title: "提交", files: "源文件", choose: "选择文件", drop: "或把文件拖到这里", problem: "题目", language: "语言",
    select: "请选择", submit: "提交", confirm: "确认提交题目 {p}（语言：{l}）？", success: "提交成功",
    oneFile: "一次提交一个源文件（.c / .cpp）", tooLarge: "源文件超过 {n} KB", oiNote: "OI 赛制：每题以最后一次提交为准，比赛结束后统一评测。",
  },
  refused: {
    not_running: "比赛未在进行中", unknown_problem: "未知题目", unsupported_language: "只接受 C / C++",
    unsupported_problem: "本题不支持在线提交", too_large: "源文件超过 {max_kb} KB", too_soon: "提交过于频繁，请 {wait_s} 秒后再试",
    empty: "源文件为空", disabled: "账号已被停用",
  },
  team: {
    submissions: "提交记录", clarifications: "答疑", requests: "提问", requestClar: "提问", noSubmissions: "还没有提交",
    noClarifications: "暂无答疑", noRequests: "还没有提问", time: "时间", problem: "题目", lang: "语言", result: "结果",
    score: "得分", from: "来自", to: "发给", subject: "主题", text: "内容", jury: "评委", all: "所有人",
    general: "一般问题", technical: "技术问题", problemSubject: "题目 {p}", rank: "名次", teamCol: "队伍",
    scoreCol: "得分", frozenRank: "封榜中，名次显示为 ?", notStarted: "比赛尚未开始",
  },
  sub: {
    title: "提交 #{id}", problem: "题目", lang: "语言", time: "提交时间", result: "结果", score: "得分",
    compile: "编译信息", source: "源代码", cases: "测试数据", runtime: "用时", memory: "内存", testcase: "测试点",
    noDetail: "比赛期间只显示结果。", subtask: "子任务 {n}",
  },
  clar: {
    request: "提问", subject: "主题", text: "内容", send: "发送", sent: "已发送", reply: "回复", replyAll: "回复所有人",
    replyTeam: "只回复该队", answer: "回复内容", question: "提问", send_to: "发给", newTitle: "发送答疑",
    newRequests: "新的提问", handled: "已处理的提问", general: "评委消息", noNew: "没有新的提问", answered: "已回复",
    unanswered: "待回复", toTeam: "队伍（用户名，留空为所有人）",
  },
  problemset: {
    title: "{c} 题目", limits: "限制：{t} 秒 / {m} MB", statement: "题面", samples: "样例", submit: "提交",
    clarifications: "{n} 条答疑", noStatement: "没有在线题面", notStarted: "比赛开始后才能看到题目", full: "满分 {n}",
    stats: "上排：各时段通过数　下排：各时段未通过数",
  },
  board: {
    started: "开始：{s} · 结束：{e}", frozen: "榜单已在比赛结束前 {m} 分钟封榜，封榜期间的提交显示为待定。",
    hidden: "成绩尚未公布", rank: "名次", team: "队伍", score: "得分", tries: "{n} 次", try1: "1 次", pending: "{a} + {p} 次",
    filter: "筛选", affiliation: "学校 / 组别", legend: "图例", solvedFirst: "首个通过", solved: "通过",
    tried: "尝试未通过", triedPending: "待定", untried: "未尝试", summary: "统计", empty: "还没有队伍",
  },
  print: {
    title: "打印", file: "文件", language: "语言", submit: "打印", success: "已提交打印任务，志愿者会送到你的座位",
    mine: "我的打印任务", done: "已打印", waiting: "等待打印", queue: "打印队列", printNow: "打印",
    markDone: "已完成", markUndone: "撤销", header: "{user} · 座位 {seat} · {time}", showDone: "显示已完成",
  },
  balloon: {
    title: "气球", done: "送达", undo: "撤销", first: "首杀", seat: "位置", team: "队伍", solved: "题目", time: "时间",
    total: "已有气球", status: "状态", showDone: "显示已送达", empty: "暂无气球",
  },
  jury: {
    title: "评委", before: "比赛前", during: "比赛中", after: "比赛后", status: "状态", judging: "评测服务",
    running: "运行中", stopped: "未运行", queue: "排队 {q} · 评测中 {r}", online: "在线 {n} 人", teamsN: "队伍 {n}",
    submissionsN: "提交 {n}", unanswered: "待回复提问 {n}", startNow: "立即开始比赛", startConfirm: "将比赛开始时间设为现在？",
    unfreeze: "解除封榜", refreeze: "恢复封榜", publish: "公布成绩", unpublish: "撤回公布", judgeOffline: "赛后评测（每题最后一次提交）",
    offlineProgress: "赛后评测：{d} / {t}", offlineDone: "赛后评测完成", reload: "重新读取 contest.toml",
    reloaded: "已重新读取", phase: { not_scheduled: "未安排", before: "未开始", running: "进行中", ended: "已结束" },
    format: { icpc: "ICPC", oi: "OI", ioi: "IOI" }, publicBoard: "公开榜单",
  },
  js: {
    title: "提交记录", newest: "最新", unjudged: "未评完", all: "全部", team: "队伍", problem: "题目", result: "结果",
    rejudge: "重新评测", rejudgeAll: "重测筛选出的全部", includeCorrect: "包括已正确的提交（仅管理员）",
    rejudged: "已重测 {n} 个提交", ignore: "忽略", unignore: "取消忽略", ignored: "已忽略", testcases: "测试点",
    verdictFor: "评测结果", judgedAt: "评测于 {t}", message: "评测信息", input: "输入", checker: "校验器信息",
  },
  acc: {
    username: "用户名", display: "显示名称", group: "学校 / 组别", seat: "座位", role: "角色", ipLock: "绑定 IP",
    disabled: "停用", password: "密码（留空自动生成）", import: "导入 CSV", importHint: "每行：用户名,显示名称,学校,座位[,密码]（可带表头）",
    create: "新建账号", reset: "重置密码", resetConfirm: "为 {u} 生成新密码？其当前登录会失效。", slips: "打印密码条",
    exportCsv: "导出 CSV", credentials: "新账号密码（只显示这一次）", deleteConfirm: "删除账号 {u}？",
    roles: { contestant: "选手", jury: "评委", balloon: "志愿者", admin: "管理员" }, slipTitle: "{title} · 登录信息", url: "网址",
  },
  cfg: {
    format: "赛制", start: "开始时间（YYYY-MM-DD HH:MM）", duration: "时长（分钟）", freeze: "封榜（结束前多少分钟，0 不封榜）",
    penalty: "罚时（分钟 / 次）", cePenalty: "编译错误计罚时", ioiScoring: "IOI 计分", maxSubtask: "各子任务取最高分",
    maxSubmission: "取单次提交最高分", oiFeedback: "OI 比赛中反馈", fbNone: "只显示已提交", fbCompile: "告知是否编译通过",
    maxSource: "源文件上限（KB）", interval: "两次提交最短间隔（秒）", bind: "监听地址（重启服务后生效）",
    publicBoard: "未登录也能查看榜单", medals: "奖牌数（金 / 银 / 铜，0 不显示）",
    problemsHint: "题目、字母和气球颜色在桌面程序的“概要”中设置（contest.toml）。",
  },
  resolver: {
    title: "滚榜", next: "下一步（空格）", auto: "自动播放", pause: "暂停", restart: "重新开始",
    remaining: "剩余 {n} 步", done: "滚榜结束", notIcpc: "滚榜只适用于 ICPC 赛制",
  },
};

type Messages = typeof zh;

const en: Messages = {
  common: {
    ok: "OK", cancel: "Cancel", save: "Save", delete: "Delete", edit: "Edit", add: "Add", close: "Close", yes: "Yes",
    no: "No", loading: "Loading…", all: "All", none: "None", actions: "Actions", confirm: "Confirm", saved: "Saved",
    copy: "Copy", copied: "Copied", download: "Download", back: "Back", search: "Search", send: "Send",
    error: "Error: {e}", filter: "Filter", apply: "Apply", reset: "Clear",
  },
  nav: {
    home: "Home", problemset: "Problemset", print: "Print", scoreboard: "Scoreboard", submit: "Submit",
    logout: "Logout", login: "Login", notifications: "Enable Notifications", jury: "Jury", submissions: "Submissions",
    clarifications: "Clarifications", balloons: "Balloons", printQueue: "Print queue", teams: "Teams",
    contest: "Contest", resolver: "Resolver", projector: "Projector",
  },
  clock: { before: "starts in {t}", left: "{t}", over: "contest over", notScheduled: "not scheduled" },
  login: { title: "Please sign in", username: "Username", password: "Password", submit: "Sign in", failed: "Invalid username or password", tooMany: "Too many attempts; try again in a minute" },
  result: {
    correct: "correct", "wrong-answer": "wrong-answer", timelimit: "timelimit", "run-error": "run-error",
    "compiler-error": "compiler-error", "no-output": "no-output", "output-limit": "output-limit", "too-late": "too-late",
    pending: "pending", submitted: "submitted", compiled: "compiled", "judge-error": "judge-error",
  },
  submit: {
    title: "Submit", files: "Source file", choose: "Choose file", drop: "or drop a file here", problem: "Problem",
    language: "Language", select: "Select", submit: "Submit", confirm: "Make submission for problem {p}, language {l}?",
    success: "Submission done!", oneFile: "One source file per submission (.c / .cpp)", tooLarge: "Source is larger than {n} KB",
    oiNote: "OI rules: the last submission per problem counts; it is judged after the contest.",
  },
  refused: {
    not_running: "The contest is not running", unknown_problem: "Unknown problem", unsupported_language: "Only C / C++ are accepted",
    unsupported_problem: "This problem cannot be submitted online", too_large: "Source is larger than {max_kb} KB",
    too_soon: "Too fast; wait {wait_s} s", empty: "Source is empty", disabled: "Account disabled",
  },
  team: {
    submissions: "Submissions", clarifications: "Clarifications", requests: "Clarification Requests",
    requestClar: "request clarification", noSubmissions: "No submissions", noClarifications: "No clarifications",
    noRequests: "No clarification requests", time: "time", problem: "problem", lang: "lang", result: "result",
    score: "score", from: "from", to: "to", subject: "subject", text: "text", jury: "Jury", all: "All",
    general: "General issue", technical: "Technical issue", problemSubject: "problem {p}", rank: "rank", teamCol: "team",
    scoreCol: "score", frozenRank: "Scoreboard frozen: rank shown as ?", notStarted: "Contest has not started",
  },
  sub: {
    title: "Submission s{id}", problem: "Problem", lang: "Language", time: "Submitted", result: "Result", score: "Score",
    compile: "Compilation output", source: "Source", cases: "Testcases", runtime: "time", memory: "memory",
    testcase: "testcase", noDetail: "Only the result is shown during the contest.", subtask: "Subtask {n}",
  },
  clar: {
    request: "Request clarification", subject: "Subject", text: "Text", send: "Send", sent: "Sent", reply: "Reply",
    replyAll: "Reply to all", replyTeam: "Reply to team", answer: "Answer", question: "Request", send_to: "Send to",
    newTitle: "Send clarification", newRequests: "New requests", handled: "Handled requests", general: "General clarifications",
    noNew: "No new requests", answered: "answered", unanswered: "unanswered", toTeam: "Team (username; empty = all)",
  },
  problemset: {
    title: "{c} problems", limits: "Limits: {t} seconds / {m} MB", statement: "statement", samples: "samples",
    submit: "Submit", clarifications: "{n} clarifications", noStatement: "No statement", notStarted: "Problems are shown once the contest starts",
    full: "{n} points", stats: "Top: accepted per interval · bottom: rejected per interval",
  },
  board: {
    started: "started: {s} - ends: {e}",
    frozen: "The scoreboard was frozen with {m} minutes remaining - solutions submitted in the last {m} minutes of the contest are still shown as pending.",
    hidden: "Results have not been published yet", rank: "rank", team: "team", score: "score", tries: "{n} tries", try1: "1 try",
    pending: "{a} + {p} tries", filter: "Filter", affiliation: "Affiliation", legend: "Legend", solvedFirst: "Solved first",
    solved: "Solved", tried: "Tried, incorrect", triedPending: "Tried, pending", untried: "Untried", summary: "Summary",
    empty: "No teams",
  },
  print: {
    title: "Print", file: "File", language: "Language", submit: "Print code", success: "Print job sent; a volunteer will bring it to your seat",
    mine: "My print jobs", done: "printed", waiting: "waiting", queue: "Print queue", printNow: "Print", markDone: "Done",
    markUndone: "Undo", header: "{user} · seat {seat} · {time}", showDone: "Show done",
  },
  balloon: {
    title: "Balloons", done: "Done", undo: "Undo", first: "first", seat: "Location", team: "Team", solved: "Solved",
    time: "Time", total: "Total", status: "Status", showDone: "Show done", empty: "No balloons",
  },
  jury: {
    title: "Jury", before: "Before contest", during: "During contest", after: "After contest", status: "Status",
    judging: "Judging", running: "running", stopped: "stopped", queue: "{q} queued · {r} judging", online: "{n} online",
    teamsN: "{n} teams", submissionsN: "{n} submissions", unanswered: "{n} unanswered requests", startNow: "Start contest now",
    startConfirm: "Set the start time to now?", unfreeze: "Unfreeze scoreboard", refreeze: "Freeze again",
    publish: "Publish results", unpublish: "Unpublish", judgeOffline: "Judge after contest (last submissions)",
    offlineProgress: "Judging: {d} / {t}", offlineDone: "Judging finished", reload: "Reload contest.toml",
    reloaded: "Reloaded", phase: { not_scheduled: "not scheduled", before: "not started", running: "running", ended: "ended" },
    format: { icpc: "ICPC", oi: "OI", ioi: "IOI" }, publicBoard: "Public scoreboard",
  },
  js: {
    title: "Submissions", newest: "newest", unjudged: "unjudged", all: "all", team: "Team", problem: "Problem", result: "Result",
    rejudge: "Rejudge", rejudgeAll: "Rejudge all shown", includeCorrect: "Include correct submissions (admin only)",
    rejudged: "{n} submissions rejudged", ignore: "Ignore", unignore: "Unignore", ignored: "ignored", testcases: "Testcases",
    verdictFor: "Judging", judgedAt: "judged {t}", message: "Message", input: "Input", checker: "Checker output",
  },
  acc: {
    username: "Username", display: "Display name", group: "Affiliation", seat: "Location", role: "Role", ipLock: "IP lock",
    disabled: "Disabled", password: "Password (empty = generate)", import: "Import CSV",
    importHint: "One per line: username,display name,affiliation,location[,password] (header allowed)", create: "Add team",
    reset: "Reset password", resetConfirm: "Generate a new password for {u}? Their sessions end.", slips: "Print password slips",
    exportCsv: "Export CSV", credentials: "New accounts (passwords are shown only once)", deleteConfirm: "Delete account {u}?",
    roles: { contestant: "Team", jury: "Jury", balloon: "Balloon runner", admin: "Admin" }, slipTitle: "{title} · login", url: "URL",
  },
  cfg: {
    format: "Format", start: "Start (YYYY-MM-DD HH:MM)", duration: "Duration (minutes)", freeze: "Freeze (minutes before end, 0 = never)",
    penalty: "Penalty time (minutes)", cePenalty: "Compile errors count", ioiScoring: "IOI scoring", maxSubtask: "Best of each subtask",
    maxSubmission: "Best single submission", oiFeedback: "OI feedback", fbNone: "Only 'submitted'", fbCompile: "Whether it compiles",
    maxSource: "Max source size (KB)", interval: "Minimum seconds between submissions", bind: "Listen address (after restart)",
    publicBoard: "Public scoreboard", medals: "Medals (gold / silver / bronze, 0 = none)",
    problemsHint: "Problems, labels and balloon colours are set in the desktop app (contest.toml).",
  },
  resolver: {
    title: "Resolver", next: "Next (space)", auto: "Auto play", pause: "Pause", restart: "Restart",
    remaining: "{n} steps left", done: "All revealed", notIcpc: "The resolver is for ICPC contests",
  },
};

function initial(): "zh-CN" | "en" {
  try {
    const s = localStorage.getItem("oij.lang");
    if (s === "en" || s === "zh-CN") return s;
  } catch {
    /* storage unavailable */
  }
  return navigator.language.toLowerCase().startsWith("zh") ? "zh-CN" : "en";
}

export const i18n = createI18n({ legacy: false, locale: initial(), fallbackLocale: "en", messages: { "zh-CN": zh, en } });

export function setLocale(l: "zh-CN" | "en") {
  i18n.global.locale.value = l;
  document.documentElement.lang = l;
  try {
    localStorage.setItem("oij.lang", l);
  } catch {
    /* ignore */
  }
}
