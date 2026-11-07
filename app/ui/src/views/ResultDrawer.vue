<script setup lang="ts">
import { computed, h, ref, watch } from "vue";
import { NDrawer, NDrawerContent, NTabs, NTabPane, NDataTable, NTag, NButton, NEmpty, NSpace, NStatistic, useMessage, type DataTableColumns } from "naive-ui";
import { useI18n } from "vue-i18n";
import { openPath } from "@tauri-apps/plugin-opener";
import { api, fmtScore, type ProblemResult, type SourceFile, type Verdict } from "../api";
import { state, judging, getCell } from "../store";

const props = defineProps<{ target: { contestant: string; problem: string } | null }>();
const emit = defineEmits<{ close: [] }>();
const { t } = useI18n();
const message = useMessage();
const result = ref<ProblemResult | null>(null);
const sources = ref<SourceFile[]>([]);
const loading = ref(false);
const tab = ref("cases");

const problem = computed(() => state.contest?.problem.find((p) => p.name === props.target?.problem));

async function load() {
  if (!props.target) return;
  loading.value = true;
  try {
    const { contestant, problem } = props.target;
    [result.value, sources.value] = await Promise.all([api.getResult(contestant, problem), api.readSource(contestant, problem)]);
    if (result.value && result.value.status !== "Judged") tab.value = result.value.status === "CompileError" ? "compile" : "cases";
  } finally {
    loading.value = false;
  }
}
watch(() => props.target, load, { immediate: true });
// Refresh when this cell is re-judged.
watch(
  () => (props.target ? getCell(props.target.contestant, props.target.problem) : undefined),
  (c, old) => {
    if (c && old && c !== old) load();
  },
);

const verdictType: Record<Verdict, "success" | "warning" | "error" | "default" | "info"> = {
  AC: "success", PC: "warning", WA: "error", TLE: "error", MLE: "error", OLE: "error", RE: "error",
  NoOutput: "default", Skipped: "default", SystemError: "info",
};

interface CaseRow {
  key: string;
  subtask: number;
  input: string;
  verdict: Verdict;
  ratio: number;
  time_ms: number;
  memory_kb: number;
  message?: string;
  first: boolean;
  span: number;
  subScore: string;
}

const caseRows = computed<CaseRow[]>(() => {
  const r = result.value;
  if (!r) return [];
  const out: CaseRow[] = [];
  r.subtasks.forEach((s, i) =>
    s.cases.forEach((c, j) =>
      out.push({
        key: `${i}-${j}`, subtask: i + 1, ...c, first: j === 0, span: s.cases.length,
        subScore: `${fmtScore(s.score)} / ${fmtScore(s.full)}`,
      }),
    ),
  );
  return out;
});

const columns = computed<DataTableColumns<CaseRow>>(() => [
  {
    title: "#", key: "subtask", width: 90, align: "center",
    rowSpan: (r) => (r.first ? r.span : 1),
    render: (r) => h("div", [h("div", { style: "font-weight:600" }, `${r.subtask}`), h("div", { class: "tabular", style: "font-size:11px;opacity:.65" }, r.subScore)]),
  },
  { title: t("detail.input"), key: "input", width: 170, ellipsis: { tooltip: true } },
  {
    title: t("detail.verdict"), key: "verdict", width: 110,
    render: (r) => h(NTag, { size: "small", type: verdictType[r.verdict], bordered: false }, () => t(`verdict.${r.verdict}`)),
  },
  { title: t("problems.time"), key: "time_ms", width: 80, align: "right", render: (r) => (r.verdict === "Skipped" ? "" : `${r.time_ms}`) },
  {
    title: t("detail.memory"), key: "memory_kb", width: 90, align: "right",
    render: (r) => (r.verdict === "Skipped" || !r.memory_kb ? "" : `${(r.memory_kb / 1024).toFixed(1)}`),
  },
  { title: t("detail.message"), key: "message", minWidth: 200, render: (r) => h("span", { class: "mono", style: "font-size:12px;white-space:pre-wrap" }, r.message ?? "") },
]);

function rejudge() {
  if (!props.target) return;
  api.startJudge({ pairs: [[props.target.contestant, props.target.problem]] }).catch((e) => message.error(String(e)));
}

function openSourceDir() {
  const f = sources.value[0]?.path;
  if (f) openPath(f.replace(/[\\/][^\\/]*$/, ""));
}
</script>

<template>
  <n-drawer :show="!!target" :width="'min(980px, 92vw)'" @update:show="(v: boolean) => !v && emit('close')">
    <n-drawer-content v-if="target" closable :native-scrollbar="false">
      <template #header>
        <div style="display: flex; align-items: center; gap: 12px">
          <span>{{ target.contestant }} · {{ problem?.title || target.problem }}</span>
        </div>
      </template>
      <n-space align="center" justify="space-between" style="margin-bottom: 12px">
        <n-space align="center" :size="28">
          <n-statistic :label="t('detail.score')">
            <span class="tabular">{{ result ? fmtScore(result.score) : "–" }}</span>
            <template #suffix><span style="font-size: 14px; opacity: 0.6"> / {{ result ? fmtScore(result.full) : "–" }}</span></template>
          </n-statistic>
          <n-statistic v-if="result" :label="t('results.time')">
            <span class="tabular">{{ result.time_ms }}</span><template #suffix><span style="font-size: 14px"> ms</span></template>
          </n-statistic>
          <n-tag v-if="result" :bordered="false" size="large">{{ t(`status.${result.status}`) }}</n-tag>
        </n-space>
        <n-space>
          <n-button v-if="sources.length" size="small" @click="openSourceDir">{{ t("common.openFolder") }}</n-button>
          <n-button size="small" type="primary" :disabled="judging" @click="rejudge">{{ t("detail.rejudge") }}</n-button>
        </n-space>
      </n-space>
      <div v-if="result" style="font-size: 12px; opacity: 0.55; margin-bottom: 8px">
        {{ t("detail.judgedAt", { t: new Date(result.judged_at * 1000).toLocaleString() }) }}
      </div>

      <n-tabs v-model:value="tab" type="segment" size="small">
        <n-tab-pane name="cases" :tab="t('detail.cases')">
          <n-empty v-if="!result" :description="t('detail.noResult')" style="margin: 40px 0" />
          <pre v-else-if="result.status !== 'Judged'" class="code">{{ result.message || t("detail.empty") }}</pre>
          <n-data-table v-else :columns="columns" :data="caseRows" :row-key="(r: CaseRow) => r.key" size="small" :bordered="true" single-line />
        </n-tab-pane>
        <n-tab-pane name="compile" :tab="t('detail.compile')">
          <pre class="code">{{ result?.message || t("detail.empty") }}</pre>
        </n-tab-pane>
        <n-tab-pane name="source" :tab="t('detail.source')">
          <n-empty v-if="!sources.length" :description="t('detail.noSource')" style="margin: 40px 0" />
          <div v-for="s in sources" :key="s.path" style="margin-bottom: 12px">
            <div class="mono" style="font-size: 12px; opacity: 0.6; margin-bottom: 4px">{{ s.path }}</div>
            <pre class="code">{{ s.text }}</pre>
          </div>
        </n-tab-pane>
      </n-tabs>
    </n-drawer-content>
  </n-drawer>
</template>
