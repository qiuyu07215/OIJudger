<script setup lang="ts">
import { computed, h, ref, watch } from "vue";
import {
  NButton, NButtonGroup, NInput, NInputNumber, NSelect, NSwitch, NForm, NFormItem, NGrid, NFormItemGi, NDataTable,
  NEmpty, NSpace, NDivider, NModal, NCard, NTag, NPopconfirm, NScrollbar, NCheckbox, NTree, NIcon, NTooltip, useMessage,
  type DataTableColumns, type TreeOption,
} from "naive-ui";
import { Wand, Plus, ArrowUp, ArrowDown, Trash, FilePlus } from "@vicons/tabler";
import { useI18n } from "vue-i18n";
import { api, fmtScore, fullScore, caseCount, type Contest, type Problem, type Subtask } from "../api";
import { state, saveContest, judging } from "../store";

const { t } = useI18n();
const message = useMessage();

const draft = ref<Contest | null>(null);
const current = ref(0);
const checked = ref<number[]>([]);
const expanded = ref<number[]>([]);
const saving = ref(false);
/** What the tree has selected: the contest, a problem, or one test point. */
const sel = ref<{ kind: "contest" } | { kind: "problem" } | { kind: "subtask"; s: number }>({ kind: "problem" });

const clone = <T,>(x: T): T => JSON.parse(JSON.stringify(x));
watch(() => state.contest, (c) => { if (c) { draft.value = clone(c); current.value = Math.min(current.value, Math.max(0, c.problem.length - 1)); } }, { immediate: true });
const dirty = computed(() => !!draft.value && !!state.contest && JSON.stringify(draft.value) !== JSON.stringify(state.contest));
const prob = computed<Problem | undefined>(() => draft.value?.problem[current.value]);

async function save() {
  if (!draft.value) return;
  saving.value = true;
  try {
    await saveContest(draft.value);
    message.success(t("msg.savedProblems"));
  } catch (e) {
    message.error(t("msg.saveFailed", { e: String(e) }), { duration: 8000 });
  } finally {
    saving.value = false;
  }
}

function revert() {
  if (state.contest) draft.value = clone(state.contest);
}

defineExpose({ detect });

const sub = computed<Subtask | undefined>(() => (sel.value.kind === "subtask" ? prob.value?.subtask[sel.value.s] : undefined));

// ---- tree ------------------------------------------------------------------------
const treeData = computed<TreeOption[]>(() => [
  {
    key: "contest",
    label: draft.value?.title || t("problems.contest"),
    prefix: () => h("span", { class: "tree-badge" }, t("problems.contestShort")),
    children: (draft.value?.problem ?? []).map((p, i) => ({
      key: `p:${i}`,
      label: p.title || p.name,
      suffix: () => h("span", { class: "tree-meta" }, `${fmtScore(fullScore(p))}`),
      children: p.subtask.map((st, j) => ({
        key: `s:${i}:${j}`,
        label: `${t("problems.point", { n: j + 1 })}`,
        suffix: () => h("span", { class: "tree-meta" }, `${fmtScore(st.score)} · ${st.cases.length}`),
      })),
    })),
  },
]);
const selectedKeys = computed(() => [
  sel.value.kind === "contest" ? "contest" : sel.value.kind === "problem" ? `p:${current.value}` : `s:${current.value}:${sel.value.s}`,
]);
const expandedKeys = ref<string[]>(["contest", "p:0"]);
function onSelect(keys: string[]) {
  const k = keys[0];
  if (!k) return;
  if (k === "contest") sel.value = { kind: "contest" };
  else if (k.startsWith("p:")) {
    current.value = Number(k.slice(2));
    sel.value = { kind: "problem" };
  } else {
    const [, pi, si] = k.split(":");
    current.value = Number(pi);
    sel.value = { kind: "subtask", s: Number(si) };
  }
  if (!expandedKeys.value.includes(`p:${current.value}`)) expandedKeys.value = [...expandedKeys.value, `p:${current.value}`];
}

function addSubtaskSelected() {
  if (!prob.value) return;
  addSubtask();
  sel.value = { kind: "subtask", s: prob.value.subtask.length - 1 };
  if (!expandedKeys.value.includes(`p:${current.value}`)) expandedKeys.value = [...expandedKeys.value, `p:${current.value}`];
}

function moveSelected(d: number) {
  if (sel.value.kind === "subtask") {
    const j = sel.value.s + d;
    if (!prob.value || j < 0 || j >= prob.value.subtask.length) return;
    moveSubtask(sel.value.s, d);
    sel.value = { kind: "subtask", s: j };
  } else if (sel.value.kind === "problem") moveProblem(d);
}

function removeSelected() {
  if (sel.value.kind === "subtask") {
    prob.value?.subtask.splice(sel.value.s, 1);
    for (const st of prob.value?.subtask ?? []) st.depends = undefined;
    sel.value = { kind: "problem" };
  } else if (sel.value.kind === "problem") removeProblem();
}

const subDepends = computed({
  get: () => (sub.value?.depends ?? []).join(","),
  set: (v: string) => {
    if (!sub.value || sel.value.kind !== "subtask") return;
    const max = sel.value.s;
    const d = v.split(/[,，\s]+/).map(Number).filter((x) => Number.isInteger(x) && x >= 1 && x <= max);
    sub.value.depends = d.length ? d : undefined;
  },
});

async function detect() {
  if (!draft.value) return;
  const found = await api.detectProblems(1000, 512);
  const have = new Set(draft.value.problem.map((p) => p.name.toLowerCase()));
  const fresh = found.filter((p) => !have.has(p.name.toLowerCase()));
  draft.value.problem.push(...fresh);
  if (fresh.length) {
    current.value = draft.value.problem.length - fresh.length;
    message.success(t("problems.detected", { n: fresh.length }));
  } else message.info(t("problems.noneDetected"));
}

function newProblem(): Problem {
  let i = 1;
  const names = new Set(draft.value?.problem.map((p) => p.name));
  while (names.has(`problem${i}`)) i++;
  return { name: `problem${i}`, kind: "traditional", subfolder: "auto", stdio: false, comparator: "lines", real_eps: 1e-6, subtask: [] };
}

function addProblem() {
  draft.value?.problem.push(newProblem());
  current.value = (draft.value?.problem.length ?? 1) - 1;
  sel.value = { kind: "problem" };
}

function removeProblem() {
  draft.value?.problem.splice(current.value, 1);
  current.value = Math.max(0, current.value - 1);
}

function moveProblem(d: number) {
  const ps = draft.value?.problem;
  const j = current.value + d;
  if (!ps || j < 0 || j >= ps.length) return;
  [ps[current.value], ps[j]] = [ps[j], ps[current.value]];
  current.value = j;
}

watch(current, () => { checked.value = []; expanded.value = []; });

// ----- optional sub-objects ------------------------------------------------
const checkerPath = computed({
  get: () => prob.value?.checker?.path ?? "",
  set: (v: string) => { if (!prob.value) return; prob.value.checker = v.trim() ? { path: v.trim(), kind: prob.value.checker?.kind ?? "testlib" } : null; },
});
const checkerKind = computed({
  get: () => prob.value?.checker?.kind ?? "testlib",
  set: (v) => { if (prob.value?.checker) prob.value.checker.kind = v; },
});
function ensureGrader() {
  if (prob.value && !prob.value.grader) prob.value.grader = { header_path: "", header_name: "", grader_path: "" };
  return prob.value!.grader!;
}
const commSources = computed({
  get: () => prob.value?.communication?.sources.join("\n") ?? "",
  set: (v: string) => { if (!prob.value) return; prob.value.communication = { sources: lines(v), interfaces: prob.value.communication?.interfaces ?? [] }; },
});
const commInterfaces = computed({
  get: () => prob.value?.communication?.interfaces.join("\n") ?? "",
  set: (v: string) => { if (!prob.value) return; prob.value.communication = { sources: prob.value.communication?.sources ?? [], interfaces: lines(v) }; },
});
const lines = (s: string) => s.split(/\r?\n/).map((x) => x.trim()).filter(Boolean);

const kindOptions = computed(() => (["traditional", "answer", "interactive", "communication"] as const).map((k) => ({ label: t(`kind.${k}`), value: k })));
const subfolderOptions = computed(() => (["auto", "inside", "outside"] as const).map((k) => ({ label: t(`subfolder.${k}`), value: k })));
const comparatorOptions = computed(() => (["lines", "strict", "tokens", "real", "binary"] as const).map((k) => ({ label: t(`comparator.${k}`), value: k })));
const checkerKindOptions = computed(() => (["testlib", "lemon"] as const).map((k) => ({ label: t(`checkerKind.${k}`), value: k })));

// ----- subtasks ------------------------------------------------------------
const bulkTime = ref<number | null>(null);
const bulkMemory = ref<number | null>(null);
const bulkTotal = ref<number>(100);

function applyBulk() {
  for (const s of prob.value?.subtask ?? []) {
    if (bulkTime.value) s.time_ms = bulkTime.value;
    if (bulkMemory.value) s.memory_mb = bulkMemory.value;
  }
}

function splitScores() {
  const st = prob.value?.subtask;
  if (!st?.length) return;
  const n = st.length;
  const base = Math.floor(bulkTotal.value / n);
  const rem = bulkTotal.value - base * n;
  st.forEach((s, i) => (s.score = base + (i >= n - rem ? 1 : 0)));
}

function addSubtask() {
  const st = prob.value!.subtask;
  const last = st[st.length - 1];
  st.push({ score: 10, time_ms: last?.time_ms ?? 1000, memory_mb: last?.memory_mb ?? 512, cases: [] });
}

function mergeChecked() {
  const st = prob.value?.subtask;
  if (!st || checked.value.length < 2) return;
  const idx = [...checked.value].sort((a, b) => a - b);
  const first = st[idx[0]];
  for (const i of idx.slice(1)) { first.cases.push(...st[i].cases); first.score += st[i].score; }
  for (const i of idx.slice(1).reverse()) st.splice(i, 1);
  for (const s of st) s.depends = undefined; // indices changed
  checked.value = [];
}

function splitChecked() {
  const st = prob.value?.subtask;
  if (!st || !checked.value.length) return;
  for (const i of [...checked.value].sort((a, b) => b - a)) {
    const s = st[i];
    if (s.cases.length < 2) continue;
    const n = s.cases.length;
    const parts: Subtask[] = s.cases.map((c, k) => ({
      score: Math.floor(s.score / n) + (k >= n - (s.score - Math.floor(s.score / n) * n) ? 1 : 0),
      time_ms: s.time_ms, memory_mb: s.memory_mb, cases: [c],
    }));
    st.splice(i, 1, ...parts);
  }
  for (const s of st) s.depends = undefined;
  checked.value = [];
}

function moveSubtask(i: number, d: number) {
  const st = prob.value!.subtask;
  const j = i + d;
  if (j < 0 || j >= st.length) return;
  [st[i], st[j]] = [st[j], st[i]];
  for (const s of st) s.depends = undefined;
}

interface SubRow { idx: number; s: Subtask }
const subRows = computed<SubRow[]>(() => (prob.value?.subtask ?? []).map((s, idx) => ({ idx, s })));

const subColumns = computed<DataTableColumns<SubRow>>(() => [
  { type: "selection", width: 36 },
  {
    type: "expand", width: 32,
    renderExpand: (r) => h("div", { style: "padding: 4px 0 4px 8px" }, [
      ...r.s.cases.map((c, k) =>
        h("div", { style: "display:flex;gap:6px;margin-bottom:4px;align-items:center" }, [
          h(NInput, { size: "tiny", value: c.input, placeholder: t("problems.inputPath"), "onUpdate:value": (v: string) => (c.input = v), style: "flex:1" }),
          h(NInput, { size: "tiny", value: c.output, placeholder: t("problems.outputPath"), "onUpdate:value": (v: string) => (c.output = v), style: "flex:1" }),
          h(NButton, { size: "tiny", quaternary: true, onClick: () => r.s.cases.splice(k, 1) }, () => "✕"),
        ]),
      ),
      h(NButton, { size: "tiny", dashed: true, onClick: () => r.s.cases.push({ input: "", output: "" }) }, () => t("problems.addCase")),
    ]),
  },
  { title: "#", key: "idx", width: 44, render: (r) => r.idx + 1 },
  {
    title: t("problems.score"), key: "score", width: 110,
    render: (r) => h(NInputNumber, { size: "tiny", value: r.s.score, min: 0, showButton: false, "onUpdate:value": (v: number | null) => (r.s.score = v ?? 0) }),
  },
  {
    title: t("problems.time"), key: "time_ms", width: 110,
    render: (r) => h(NInputNumber, { size: "tiny", value: r.s.time_ms, min: 1, showButton: false, "onUpdate:value": (v: number | null) => (r.s.time_ms = v ?? 1000) }),
  },
  {
    title: t("problems.memory"), key: "memory_mb", width: 110,
    render: (r) => h(NInputNumber, { size: "tiny", value: r.s.memory_mb, min: 1, showButton: false, "onUpdate:value": (v: number | null) => (r.s.memory_mb = v ?? 256) }),
  },
  {
    title: t("problems.depends"), key: "depends", width: 110,
    render: (r) => h(NInput, {
      size: "tiny", value: (r.s.depends ?? []).join(","), placeholder: "1,2",
      "onUpdate:value": (v: string) => { const d = v.split(/[,，\s]+/).map(Number).filter((x) => Number.isInteger(x) && x >= 1 && x <= r.idx); r.s.depends = d.length ? d : undefined; },
    }),
  },
  {
    title: t("problems.dataFiles"), key: "cases", minWidth: 180, ellipsis: { tooltip: true },
    render: (r) => r.s.cases.length ? `${r.s.cases.map((c) => c.input.split("/").pop()).join(", ")}` : h(NTag, { size: "tiny", type: "warning" }, () => t("problems.cases", { n: 0 })),
  },
  {
    title: "", key: "ops", width: 96,
    render: (r) => h(NButtonGroup, { size: "tiny" }, () => [
      h(NButton, { onClick: () => moveSubtask(r.idx, -1) }, () => "↑"),
      h(NButton, { onClick: () => moveSubtask(r.idx, 1) }, () => "↓"),
      h(NButton, { onClick: () => prob.value!.subtask.splice(r.idx, 1) }, () => "✕"),
    ]),
  },
]);

// ----- add by pattern ----------------------------------------------------------
const showPattern = ref(false);
const patDir = ref("");
const patInput = ref("");
const patOutExt = ref("out;ans");
const patReplace = ref(true);
const patFiles = ref<string[]>([]);

async function openPattern() {
  if (!prob.value) return;
  patDir.value = prob.value.name;
  patInput.value = `${prob.value.name}(\\d+)\\.in`;
  showPattern.value = true;
  await loadPatFiles();
}
async function loadPatFiles() {
  patFiles.value = await api.listDataFiles(patDir.value).catch(() => []);
}
const patPreview = computed(() => {
  let re: RegExp;
  try { re = new RegExp(`^${patInput.value}$`); } catch { return { groups: [] as { key: string; cases: { input: string; output: string }[] }[], error: true }; }
  const exts = patOutExt.value.split(/[;,\s]+/).filter(Boolean);
  const set = new Set(patFiles.value);
  const groups = new Map<string, { input: string; output: string }[]>();
  for (const f of patFiles.value) {
    const m = re.exec(f);
    if (!m) continue;
    const stem = f.replace(/\.[^.]*$/, "");
    const out = exts.map((e) => `${stem}.${e}`).find((o) => set.has(o));
    if (!out) continue;
    const key = m[1] ?? f;
    const prefix = patDir.value ? `${patDir.value.replace(/[\\/]+$/, "")}/` : "";
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key)!.push({ input: prefix + f, output: prefix + out });
  }
  const coll = new Intl.Collator(undefined, { numeric: true });
  const list = [...groups.entries()].sort((a, b) => coll.compare(a[0], b[0])).map(([key, cases]) => ({ key, cases: cases.sort((a, b) => coll.compare(a.input, b.input)) }));
  return { groups: list, error: false };
});
function applyPattern() {
  const p = prob.value;
  if (!p) return;
  const g = patPreview.value.groups;
  if (!g.length) return;
  const time = p.subtask[0]?.time_ms ?? 1000;
  const mem = p.subtask[0]?.memory_mb ?? 512;
  const subs: Subtask[] = g.map((x) => ({ score: 0, time_ms: time, memory_mb: mem, cases: x.cases }));
  if (patReplace.value) p.subtask = subs; else p.subtask.push(...subs);
  bulkTotal.value = 100;
  splitScores();
  showPattern.value = false;
}
</script>

<template>
  <div v-if="draft" class="wrap">
    <div class="side">
      <div class="treebar">
        <n-tooltip><template #trigger><n-button size="tiny" quaternary @click="detect"><template #icon><n-icon><Wand /></n-icon></template></n-button></template>{{ t("problems.detect") }}</n-tooltip>
        <n-tooltip><template #trigger><n-button size="tiny" quaternary @click="addProblem"><template #icon><n-icon><Plus /></n-icon></template></n-button></template>{{ t("problems.add") }}</n-tooltip>
        <n-tooltip><template #trigger><n-button size="tiny" quaternary :disabled="!prob" @click="addSubtaskSelected"><template #icon><n-icon><FilePlus /></n-icon></template></n-button></template>{{ t("problems.addSubtask") }}</n-tooltip>
        <n-tooltip><template #trigger><n-button size="tiny" quaternary :disabled="sel.kind === 'contest'" @click="moveSelected(-1)"><template #icon><n-icon><ArrowUp /></n-icon></template></n-button></template>{{ t("common.up") }}</n-tooltip>
        <n-tooltip><template #trigger><n-button size="tiny" quaternary :disabled="sel.kind === 'contest'" @click="moveSelected(1)"><template #icon><n-icon><ArrowDown /></n-icon></template></n-button></template>{{ t("common.down") }}</n-tooltip>
        <n-popconfirm @positive-click="removeSelected">
          <template #trigger><n-button size="tiny" quaternary :disabled="sel.kind === 'contest'"><template #icon><n-icon><Trash /></n-icon></template></n-button></template>
          {{ sel.kind === "subtask" ? t("problems.removePoint") : t("problems.removeConfirm", { name: prob?.name ?? "" }) }}
        </n-popconfirm>
      </div>
      <n-scrollbar style="flex: 1">
        <n-tree
          block-line
          :data="treeData"
          :selected-keys="selectedKeys"
          v-model:expanded-keys="expandedKeys"
          :cancelable="false"
          style="padding: 4px 6px"
          @update:selected-keys="onSelect"
        />
      </n-scrollbar>
      <div class="savebar">
        <div v-if="dirty" style="font-size: 12px; color: var(--accent); margin-bottom: 6px">● {{ t("common.unsaved") }}</div>
        <n-space :size="8">
          <n-button type="primary" :disabled="!dirty || judging" :loading="saving" @click="save">{{ t("common.save") }}</n-button>
          <n-button :disabled="!dirty" @click="revert">{{ t("common.revert") }}</n-button>
        </n-space>
      </div>
    </div>

    <n-scrollbar style="flex: 1">
      <div v-if="sel.kind === 'contest'" class="editor">
        <h2 class="serif heading">{{ t("problems.contest") }}</h2>
        <n-form label-placement="top" size="small" :show-feedback="false" style="max-width: 520px">
          <n-form-item :label="t('problems.contestTitle')"><n-input v-model:value="draft.title" /></n-form-item>
        </n-form>
        <div class="summary">
          <div><b class="tabular">{{ draft.problem.length }}</b> {{ t("problems.problemsUnit") }}</div>
          <div><b class="tabular">{{ fmtScore(draft.problem.reduce((a, p) => a + fullScore(p), 0)) }}</b> {{ t("problems.pointsUnit") }}</div>
          <div><b class="tabular">{{ state.contestants.length }}</b> {{ t("problems.contestantsUnit") }}</div>
        </div>
        <div class="hint">{{ t("problems.contestHint") }}</div>
      </div>
      <n-empty v-else-if="!prob" :description="t('problems.none')" style="margin-top: 100px" />
      <div v-else-if="sel.kind === 'subtask' && sub" class="editor">
        <h2 class="serif heading">{{ prob.title || prob.name }} · {{ t("problems.point", { n: sel.s + 1 }) }}</h2>
        <n-form label-placement="top" size="small" :show-feedback="false">
          <n-grid :cols="4" :x-gap="16">
            <n-form-item-gi :label="t('problems.score')"><n-input-number v-model:value="sub.score" :min="0" /></n-form-item-gi>
            <n-form-item-gi :label="t('problems.time')"><n-input-number v-model:value="sub.time_ms" :min="1" :step="100" /></n-form-item-gi>
            <n-form-item-gi :label="t('problems.memory')"><n-input-number v-model:value="sub.memory_mb" :min="1" :step="64" /></n-form-item-gi>
            <n-form-item-gi :label="t('problems.depends')"><n-input v-model:value="subDepends" placeholder="1,2" /></n-form-item-gi>
          </n-grid>
        </n-form>
        <n-divider title-placement="left">{{ t("problems.dataFiles") }} · {{ t("problems.cases", { n: sub.cases.length }) }}</n-divider>
        <div class="hint" style="margin-bottom: 8px">{{ t("problems.minHint") }}</div>
        <div v-for="(c, k) in sub.cases" :key="k" class="case-row">
          <span class="tabular idx">{{ k + 1 }}</span>
          <n-input v-model:value="c.input" size="small" :placeholder="t('problems.inputPath')" class="mono" />
          <n-input v-model:value="c.output" size="small" :placeholder="t('problems.outputPath')" class="mono" />
          <n-button size="small" quaternary @click="sub.cases.splice(k, 1)"><template #icon><n-icon><Trash /></n-icon></template></n-button>
        </div>
        <n-button size="small" dashed style="margin-top: 6px" @click="sub.cases.push({ input: '', output: '' })">＋ {{ t("problems.addCase") }}</n-button>
      </div>
      <div v-else class="editor">
        <h2 class="serif heading">{{ prob.title || prob.name }}</h2>
        <n-form label-placement="top" size="small" :show-feedback="false">
          <n-divider title-placement="left">{{ t("problems.basics") }}</n-divider>
          <n-grid :cols="3" :x-gap="16" :y-gap="12">
            <n-form-item-gi :label="t('problems.name')"><n-input v-model:value="prob.name" /></n-form-item-gi>
            <n-form-item-gi :label="t('problems.title')"><n-input v-model:value="prob.title" :placeholder="prob.name" /></n-form-item-gi>
            <n-form-item-gi :label="t('problems.kind')"><n-select v-model:value="prob.kind" :options="kindOptions" /></n-form-item-gi>
            <n-form-item-gi v-if="prob.kind !== 'communication'" :label="t('problems.sourceName')">
              <n-input v-model:value="prob.source_name" :placeholder="prob.name" />
            </n-form-item-gi>
            <n-form-item-gi :label="t('problems.subfolder')"><n-select v-model:value="prob.subfolder" :options="subfolderOptions" /></n-form-item-gi>
            <n-form-item-gi v-if="prob.kind !== 'answer'" :label="t('problems.compileArgs')">
              <n-input v-model:value="prob.compile_args" placeholder="-DLOCAL" class="mono" />
            </n-form-item-gi>
            <n-form-item-gi v-if="prob.kind === 'answer'" :label="t('problems.answerExt')">
              <n-input v-model:value="prob.answer_ext" placeholder="out" />
            </n-form-item-gi>
          </n-grid>

          <template v-if="prob.kind !== 'answer'">
            <n-divider title-placement="left">{{ t("problems.io") }}</n-divider>
            <n-grid :cols="3" :x-gap="16" :y-gap="12">
              <n-form-item-gi :label="t('problems.stdio')"><n-switch v-model:value="prob.stdio" /></n-form-item-gi>
              <n-form-item-gi v-if="!prob.stdio" :label="t('problems.inputFile')">
                <n-input v-model:value="prob.input_file" :placeholder="`${prob.name}.in`" />
              </n-form-item-gi>
              <n-form-item-gi v-if="!prob.stdio" :label="t('problems.outputFile')">
                <n-input v-model:value="prob.output_file" :placeholder="`${prob.name}.out`" />
              </n-form-item-gi>
            </n-grid>
          </template>

          <template v-if="prob.kind === 'interactive'">
            <n-divider title-placement="left">{{ t("problems.grader") }}</n-divider>
            <n-grid :cols="3" :x-gap="16" :y-gap="12">
              <n-form-item-gi :label="t('problems.headerPath')">
                <n-input :value="prob.grader?.header_path" @update:value="(v: string) => (ensureGrader().header_path = v)" />
              </n-form-item-gi>
              <n-form-item-gi :label="t('problems.headerName')">
                <n-input :value="prob.grader?.header_name" placeholder="interaction.h" @update:value="(v: string) => (ensureGrader().header_name = v)" />
              </n-form-item-gi>
              <n-form-item-gi :label="t('problems.graderPath')">
                <n-input :value="prob.grader?.grader_path" @update:value="(v: string) => (ensureGrader().grader_path = v)" />
              </n-form-item-gi>
              <n-form-item-gi :span="3" :label="t('problems.interactor')">
                <n-input :value="prob.interactor ?? ''" placeholder="guess/interactor.cpp" @update:value="(v: string) => (prob!.interactor = v.trim() || null)" />
              </n-form-item-gi>
            </n-grid>
          </template>

          <template v-if="prob.kind === 'communication'">
            <n-divider title-placement="left">{{ t("problems.comm") }}</n-divider>
            <n-grid :cols="2" :x-gap="16">
              <n-form-item-gi :label="t('problems.commSources')"><n-input v-model:value="commSources" type="textarea" :rows="3" class="mono" /></n-form-item-gi>
              <n-form-item-gi :label="t('problems.commInterfaces')"><n-input v-model:value="commInterfaces" type="textarea" :rows="3" class="mono" /></n-form-item-gi>
            </n-grid>
          </template>

          <n-divider title-placement="left">{{ t("problems.judge") }}</n-divider>
          <n-grid :cols="3" :x-gap="16" :y-gap="12">
            <n-form-item-gi :label="t('problems.checker')"><n-input v-model:value="checkerPath" placeholder="real/checker.cpp" /></n-form-item-gi>
            <n-form-item-gi v-if="prob.checker" :label="t('problems.checkerKind')"><n-select v-model:value="checkerKind" :options="checkerKindOptions" /></n-form-item-gi>
            <n-form-item-gi v-if="!prob.checker" :label="t('problems.comparator')"><n-select v-model:value="prob.comparator" :options="comparatorOptions" /></n-form-item-gi>
            <n-form-item-gi v-if="!prob.checker && prob.comparator === 'real'" :label="t('problems.eps')">
              <n-input-number v-model:value="prob.real_eps" :min="0" :step="0.000001" />
            </n-form-item-gi>
          </n-grid>
        </n-form>

        <n-divider title-placement="left">
          {{ t("problems.subtasks") }} · {{ t("problems.total", { n: fmtScore(fullScore(prob)) }) }} · {{ t("problems.cases", { n: caseCount(prob) }) }}
        </n-divider>
        <n-space align="center" :size="8" style="margin-bottom: 8px" :wrap="true">
          <n-button size="small" @click="addSubtask">＋ {{ t("problems.addSubtask") }}</n-button>
          <n-button size="small" @click="openPattern">{{ t("problems.pattern") }}</n-button>
          <n-button size="small" :disabled="checked.length < 2" @click="mergeChecked">{{ t("problems.merge") }}</n-button>
          <n-button size="small" :disabled="!checked.length" @click="splitChecked">{{ t("problems.split") }}</n-button>
          <span style="opacity: 0.6; font-size: 12px; margin-left: 8px">{{ t("problems.bulk") }}:</span>
          <n-input-number v-model:value="bulkTime" size="small" :min="1" :placeholder="t('problems.time')" style="width: 120px" :show-button="false" />
          <n-input-number v-model:value="bulkMemory" size="small" :min="1" :placeholder="t('problems.memory')" style="width: 120px" :show-button="false" />
          <n-button size="small" :disabled="!bulkTime && !bulkMemory" @click="applyBulk">{{ t("common.apply") }}</n-button>
          <n-input-number v-model:value="bulkTotal" size="small" :min="0" style="width: 90px" :show-button="false" />
          <n-button size="small" @click="splitScores">{{ t("problems.splitScores", { n: bulkTotal }) }}</n-button>
        </n-space>
        <n-data-table
          v-model:checked-row-keys="checked"
          v-model:expanded-row-keys="expanded"
          :columns="subColumns"
          :data="subRows"
          :row-key="(r: SubRow) => r.idx"
          size="small"
          :max-height="520"
          virtual-scroll
        />
        <div style="margin-top: 16px; text-align: right">
          <n-popconfirm @positive-click="removeProblem">
            <template #trigger><n-button size="small" type="error" ghost>{{ t("common.delete") }} {{ prob.name }}</n-button></template>
            {{ t("problems.removeConfirm", { name: prob.name }) }}
          </n-popconfirm>
        </div>
      </div>
    </n-scrollbar>

    <n-modal v-model:show="showPattern">
      <n-card :title="t('problems.patternTitle')" style="width: min(760px, 94vw)" closable @close="showPattern = false">
        <div style="font-size: 12px; opacity: 0.7; margin-bottom: 12px">{{ t("problems.patternHelp") }}</div>
        <n-grid :cols="3" :x-gap="12">
          <n-form-item-gi :label="t('problems.patternDir')" :show-feedback="false"><n-input v-model:value="patDir" @blur="loadPatFiles" /></n-form-item-gi>
          <n-form-item-gi :label="t('problems.patternInput')" :show-feedback="false"><n-input v-model:value="patInput" class="mono" :status="patPreview.error ? 'error' : undefined" /></n-form-item-gi>
          <n-form-item-gi :label="t('problems.patternOutput')" :show-feedback="false"><n-input v-model:value="patOutExt" /></n-form-item-gi>
        </n-grid>
        <div style="margin: 12px 0 6px; font-weight: 600">{{ t("problems.preview") }} · {{ patPreview.groups.length }}</div>
        <n-scrollbar style="max-height: 300px">
          <div v-for="(g, i) in patPreview.groups" :key="g.key" class="mono" style="font-size: 12px; padding: 2px 0">
            <b>#{{ i + 1 }}</b> {{ g.cases.map((c) => `${c.input} → ${c.output.split("/").pop()}`).join("   ") }}
          </div>
        </n-scrollbar>
        <template #footer>
          <n-space justify="space-between" align="center">
            <n-checkbox v-model:checked="patReplace">{{ t("problems.replace") }}</n-checkbox>
            <n-button type="primary" :disabled="!patPreview.groups.length" @click="applyPattern">{{ t("common.apply") }}</n-button>
          </n-space>
        </template>
      </n-card>
    </n-modal>
  </div>
</template>

<style scoped>
.wrap {
  display: flex;
  height: 100%;
}
.side {
  width: 270px;
  border-right: 1px solid var(--divider);
  background: var(--panel);
  display: flex;
  flex-direction: column;
}
.treebar {
  display: flex;
  gap: 2px;
  padding: 6px 8px;
  border-bottom: 1px solid var(--divider);
}
:deep(.tree-meta) {
  font-size: 11px;
  color: var(--text3);
  font-variant-numeric: tabular-nums;
}
:deep(.tree-badge) {
  font-size: 10px;
  padding: 0 5px;
  border-radius: 4px;
  background: var(--accent-soft);
  color: var(--accent);
}
.heading {
  font-size: 22px;
  font-weight: 500;
  margin: 16px 0 8px;
}
.summary {
  display: flex;
  gap: 28px;
  margin: 18px 0;
  color: var(--text2);
}
.summary b {
  font-size: 22px;
  color: var(--text);
  margin-right: 4px;
}
.hint {
  font-size: 12px;
  color: var(--text3);
}
.case-row {
  display: grid;
  grid-template-columns: 28px 1fr 1fr 32px;
  gap: 8px;
  align-items: center;
  margin-bottom: 6px;
}
.case-row .idx {
  color: var(--text3);
  text-align: right;
  font-size: 12px;
}
.item {
  padding: 8px 14px;
  cursor: pointer;
  border-left: 3px solid transparent;
}
.item:hover {
  background: rgba(127, 127, 127, 0.08);
}
.item.active {
  background: rgba(47, 125, 79, 0.12);
  border-left-color: #2f7d4f;
}
.sub {
  font-size: 12px;
  opacity: 0.6;
}
.savebar {
  padding: 12px;
  border-top: 1px solid var(--divider);
}
.editor {
  padding: 4px 24px 24px;
  max-width: 1100px;
}
</style>
