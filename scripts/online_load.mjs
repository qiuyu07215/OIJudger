// Load test for the OIJudger online contest server: N teams behave like the
// DOMjudge-style web pages (SSE + throttled reloads), submit in a steady
// phase and then in an end-of-contest burst; one jury client watches.
//
// node scripts/online_load.mjs <base-url> <teams.csv> <admin-password> <demo-source-dir> <out.json>
import { readFileSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";


const [BASE, CSV, ADMIN_PW, SRC, OUT] = process.argv.slice(2);
const STEADY_S = Number(process.env.STEADY_S ?? 300); // steady phase length
const STEADY_N = Number(process.env.STEADY_N ?? 3); // submissions per team in steady phase
const BURST_S = Number(process.env.BURST_S ?? 20); // burst window
const BOARD_SHARE = Number(process.env.BOARD_SHARE ?? 0.5); // teams with a scoreboard tab open
const LIMIT = Number(process.env.TEAMS ?? 200);

const t0 = Date.now();
const el = () => ((Date.now() - t0) / 1000).toFixed(1);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const log = (...a) => console.log(`[${el()}s]`, ...a);

// ---- metrics -------------------------------------------------------------------
const lat = new Map(); // endpoint -> [ms]
const errors = new Map(); // endpoint status -> count
function record(key, ms, status) {
  if (!lat.has(key)) lat.set(key, []);
  lat.get(key).push(ms);
  if (status >= 400 || status === 0) errors.set(`${key} ${status}`, (errors.get(`${key} ${status}`) ?? 0) + 1);
}
const pct = (a, p) => {
  if (!a.length) return 0;
  const s = [...a].sort((x, y) => x - y);
  return s[Math.min(s.length - 1, Math.floor((p / 100) * s.length))];
};
const summary = (a) => ({ n: a.length, p50: pct(a, 50), p95: pct(a, 95), p99: pct(a, 99), max: a.length ? Math.max(...a) : 0 });

// ---- sources -------------------------------------------------------------------
const PROBLEMS = ["sum", "fio", "real", "guess", "add"];
function source(variant, p) {
  const path = variant === "alice" ? `${SRC}/alice/${p}/${p}.cpp` : `${SRC}/${variant}/${p}.cpp`;
  return readFileSync(path, "utf8");
}
const SOURCES = {};
for (const v of ["alice", "bob", "carol"]) for (const p of PROBLEMS) SOURCES[`${v}/${p}`] = source(v, p);
function pickVariant() {
  const r = Math.random();
  return r < 0.5 ? "alice" : r < 0.85 ? "bob" : "carol";
}

// ---- client --------------------------------------------------------------------
function throttle(fn, ms) {
  let t = null;
  let last = 0;
  return () => {
    if (t) return;
    t = setTimeout(() => {
      t = null;
      last = Date.now();
      fn().catch(() => {});
    }, Math.max(0, last + ms - Date.now()));
  };
}

class Client {
  constructor(user, pw) {
    this.user = user;
    this.pw = pw;
    this.cookie = "";
    this.notices = 0;
    this.closed = false;
  }
  async req(method, path, body) {
    const key = `${method} ${path.replace(/\/\d+/g, "/{id}")}`;
    const start = performance.now();
    let status = 0;
    try {
      const res = await fetch(BASE + path, {
        method,
        headers: { cookie: this.cookie, "X-Requested-With": "oij", ...(body ? { "content-type": "application/json" } : {}) },
        body: body ? JSON.stringify(body) : undefined,
      });
      status = res.status;
      const text = await res.text();
      const sc = res.headers.get("set-cookie");
      if (sc) this.cookie = sc.split(";")[0];
      record(key, performance.now() - start, status);
      return { status, data: text ? JSON.parse(text) : null };
    } catch (e) {
      record(key, performance.now() - start, 0);
      return { status: 0, data: null, error: String(e) };
    }
  }
  async login() {
    const r = await this.req("POST", "/api/login", { username: this.user, password: this.pw });
    if (r.status !== 200) throw new Error(`login ${this.user}: ${r.status} ${JSON.stringify(r.data)}`);
  }
  async listen(onNotice) {
    const ac = new AbortController();
    this.abort = () => ac.abort();
    const run = async () => {
      while (!this.closed) {
        try {
          const res = await fetch(BASE + "/api/events", { headers: { cookie: this.cookie }, signal: ac.signal });
          const dec = new TextDecoder();
          let buf = "";
          for await (const chunk of res.body) {
            buf += dec.decode(chunk, { stream: true });
            let i;
            while ((i = buf.indexOf("\n\n")) >= 0) {
              const block = buf.slice(0, i);
              buf = buf.slice(i + 2);
              if (/^event: hello/m.test(block)) continue;
              const data = block.split("\n").filter((l) => l.startsWith("data:")).map((l) => l.slice(5).trim()).join("\n");
              if (data) {
                this.notices++;
                try {
                  onNotice(JSON.parse(data));
                } catch {}
              }
            }
          }
        } catch (e) {
          if (this.closed) return;
        }
        if (!this.closed) {
          sseDrops++;
          await sleep(1000);
        }
      }
    };
    run();
  }
}
let sseDrops = 0;

// ---- teams ---------------------------------------------------------------------
const teams = readFileSync(CSV, "utf8")
  .trim()
  .split(/\r?\n/)
  .slice(1)
  .map((l) => l.split(","))
  .slice(0, LIMIT)
  .map(([u, , , , pw]) => ({ user: u, pw }));

const submitted = new Map(); // id -> {team, variant, problem, at}
const verdictLatency = []; // ms from submit response to result seen by the team page
const seenDone = new Set();
const refused = new Map();

class Team {
  constructor(t, i) {
    this.c = new Client(t.user, t.pw);
    this.board = i < teams.length * BOARD_SHARE;
    this.subs = [];
    this.lastSubmit = 0;
    this.loadSubs = throttle(async () => {
      const r = await this.c.req("GET", "/api/submissions");
      if (r.status !== 200) return;
      this.subs = r.data;
      for (const s of r.data) {
        if (s.state === "done" && !seenDone.has(s.id) && submitted.has(s.id)) {
          seenDone.add(s.id);
          verdictLatency.push(Date.now() - submitted.get(s.id).at);
          submitted.get(s.id).label = s.label;
        }
      }
    }, 600);
    this.loadRow = throttle(() => this.c.req("GET", "/api/team-row"), 1000);
    this.loadBoard = throttle(() => this.c.req("GET", "/api/scoreboard"), 1500);
  }
  async start() {
    await this.c.login();
    await Promise.all(["/api/info", "/api/me", "/api/team-row", "/api/submissions", "/api/clarifications", "/api/announcements", "/api/problemset-stats"].map((p) => this.c.req("GET", p)));
    if (this.board) await this.c.req("GET", "/api/scoreboard");
    this.c.listen((n) => {
      if (n.type === "submission") {
        this.loadSubs();
        this.loadRow();
      } else if (n.type === "scoreboard" || n.type === "contest") {
        this.loadRow();
        if (this.board) this.loadBoard();
      }
    });
    if (this.board) this.timer = setInterval(() => this.loadBoard(), 30_000);
  }
  async submit() {
    const wait = this.lastSubmit + 10_500 - Date.now();
    if (wait > 0) await sleep(wait);
    const problem = PROBLEMS[Math.floor(Math.random() * PROBLEMS.length)];
    const variant = pickVariant();
    const src = `// ${this.c.user} ${Math.random().toString(36).slice(2)}\n` + SOURCES[`${variant}/${problem}`];
    const r = await this.c.req("POST", "/api/submit", { problem, lang: "cpp", source: src });
    this.lastSubmit = Date.now();
    if (r.status === 200) submitted.set(r.data.id, { team: this.c.user, variant, problem, at: Date.now() });
    else refused.set(`${r.status} ${r.data?.message ?? r.error}`, (refused.get(`${r.status} ${r.data?.message ?? r.error}`) ?? 0) + 1);
  }
  stop() {
    this.c.closed = true;
    this.c.abort?.();
    clearInterval(this.timer);
  }
}

// ---- process sampling ------------------------------------------------------------
const samples = [];
function sample(phase) {
  try {
    const out = execFileSync("powershell", ["-NoProfile", "-Command", "$p = Get-Process oijudger-cli | Select-Object -First 1; \"$($p.CPU) $($p.WorkingSet64) $($p.HandleCount) $($p.Threads.Count)\""], { encoding: "utf8" }).trim();
    const [cpu, ws, handles, threads] = out.split(" ").map(Number);
    samples.push({ t: Number(el()), phase, cpu, ws_mb: ws / 1048576, handles, threads });
  } catch {}
}

// ---- run -----------------------------------------------------------------------
const jury = new Client("admin", ADMIN_PW);
let queue = { queued: 0, running: 0 };
let maxQueue = 0;
const juryLoadSubs = throttle(() => jury.req("GET", "/api/submissions"), 1000);
const juryStatus = throttle(async () => {
  const r = await jury.req("GET", "/api/jury/status");
  if (r.status === 200) {
    queue = r.data;
    maxQueue = Math.max(maxQueue, r.data.queued);
  }
}, 1000);

let phase = "login";
const sampler = setInterval(() => sample(phase), 5000);
sample("idle");

await jury.login();
jury.listen((n) => {
  if (n.type === "submission") juryLoadSubs();
  if (n.type === "queue") {
    maxQueue = Math.max(maxQueue, n.queued);
    juryStatus();
  }
});

const T = teams.map((t, i) => new Team(t, i));
const loginStart = Date.now();
await Promise.all(T.map((t) => t.start()));
const loginMs = Date.now() - loginStart;
log(`${T.length} teams logged in and loaded their pages in ${loginMs} ms`);

phase = "steady";
log(`steady phase: ${STEADY_N} submissions per team over ${STEADY_S} s`);
await Promise.all(
  T.map(async (t) => {
    const times = Array.from({ length: STEADY_N }, () => Math.random() * STEADY_S * 1000).sort((a, b) => a - b);
    const base = Date.now();
    for (const at of times) {
      const w = base + at - Date.now();
      if (w > 0) await sleep(w);
      await t.submit();
    }
  }),
);
const progress = setInterval(() => log(`queue ${queue.queued} queued, ${queue.running} running; results seen ${seenDone.size}/${submitted.size}`), 15_000);
log(`steady submissions sent: ${submitted.size}; waiting for the queue to drain`);
async function drain() {
  const start = Date.now();
  for (;;) {
    const r = await jury.req("GET", "/api/jury/status");
    if (r.status === 200 && r.data.queued === 0 && r.data.running === 0) break;
    await sleep(1000);
  }
  return Date.now() - start;
}
const steadyDrain = await drain();
const steadyLat = [...verdictLatency];
log(`steady queue drained ${steadyDrain} ms after the last submission`);

phase = "burst";
await sleep(12_000);
const before = submitted.size;
const burstStart = Date.now();
await Promise.all(
  T.map(async (t) => {
    await sleep(Math.random() * BURST_S * 1000);
    await t.submit();
  }),
);
log(`burst: ${submitted.size - before} submissions within ${BURST_S} s`);
const burstSubmitted = Date.now();
const burstDrain = await drain();
const burstTotal = Date.now() - burstStart;
log(`burst queue drained ${burstDrain} ms after the last submission (${burstTotal} ms after the burst began)`);
clearInterval(progress);

phase = "settle";
await sleep(4000);
// Results the pages never saw through pushes (missed notices).
const missedBeforeReconcile = [...submitted.keys()].filter((id) => !seenDone.has(id)).length;
const final = await jury.req("GET", "/api/submissions");
const byId = new Map(final.data.map((s) => [s.id, s]));
const notDone = final.data.filter((s) => s.state !== "done");
const combos = {};
for (const [id, s] of submitted) {
  const k = `${s.variant}/${s.problem}`;
  const label = byId.get(id)?.label ?? "missing";
  combos[k] ??= {};
  combos[k][label] = (combos[k][label] ?? 0) + 1;
}
const boards = await Promise.all([jury.req("GET", "/api/scoreboard"), T[0].c.req("GET", "/api/scoreboard")]);

clearInterval(sampler);
sample("end");
for (const t of T) t.stop();
jury.closed = true;
jury.abort?.();

const report = {
  teams: T.length,
  login_and_first_load_ms: loginMs,
  submissions: submitted.size,
  refused: Object.fromEntries(refused),
  steady: { latency_ms: summary(steadyLat), drain_after_last_ms: steadyDrain },
  burst: { latency_ms: summary(verdictLatency.slice(steadyLat.length)), drain_after_last_ms: burstDrain, total_ms: burstTotal },
  max_queue: maxQueue,
  results_not_seen_via_push: missedBeforeReconcile,
  not_done_at_end: notDone.length,
  sse_reconnects: sseDrops,
  notices_per_team_avg: T.reduce((a, t) => a + t.c.notices, 0) / T.length,
  scoreboard_bytes: { jury: JSON.stringify(boards[0].data).length, team: JSON.stringify(boards[1].data).length },
  endpoints: Object.fromEntries([...lat].map(([k, v]) => [k, summary(v.map((x) => Math.round(x)))])),
  errors: Object.fromEntries(errors),
  verdicts: combos,
  process: samples,
};
writeFileSync(OUT, JSON.stringify(report, null, 2));
log("report written to", OUT);
process.exit(0);
