<script setup lang="ts">
import { computed, h, onMounted, ref, watch } from "vue";
import {
  NButton, NCard, NDataTable, NDropdown, NEmpty, NSelect, NSpin, NScrollbar, NTooltip, NCollapse, NCollapseItem,
  useMessage, useOsTheme, useThemeVars, type DataTableColumns,
} from "naive-ui";
import { useI18n } from "vue-i18n";
import { save } from "@tauri-apps/plugin-dialog";
import VChart from "vue-echarts";
import { use } from "echarts/core";
import { CanvasRenderer } from "echarts/renderers";
import { BarChart, BoxplotChart, HeatmapChart } from "echarts/charts";
import { GridComponent, TooltipComponent, VisualMapComponent } from "echarts/components";
import { api, fmtScore, type ContestStats, type ProblemStat, type ScoreCount, type GroupStat } from "../api";
import { state, cells } from "../store";

use([CanvasRenderer, BarChart, BoxplotChart, HeatmapChart, GridComponent, TooltipComponent, VisualMapComponent]);

const { t } = useI18n();
const message = useMessage();
const os = useOsTheme();
const vars = useThemeVars();
const stats = ref<ContestStats | null>(null);
const loading = ref(false);
const groupKey = ref<string | null>(null);

async function load() {
  loading.value = true;
  try {
    stats.value = await api.getStats();
    if (!groupKey.value || !stats.value.group_keys.includes(groupKey.value)) groupKey.value = stats.value.group_keys[0] ?? null;
  } catch (e) {
    message.error(String(e));
  } finally {
    loading.value = false;
  }
}
onMounted(load);
// Recompute when results change (debounced by the progress batching).
let timer: number | undefined;
watch(cells, () => {
  clearTimeout(timer);
  timer = window.setTimeout(load, 800);
});

// ---- palette (reference data-viz palette; light / dark steps) -------------
const dark = computed(() => os.value === "dark");
const blue = computed(() => (dark.value ? "#3987e5" : "#2a78d6"));
const seqRamp = computed(() => (dark.value ? ["#104281", "#256abf", "#5598e7", "#9ec5f4"] : ["#cde2fb", "#86b6ef", "#3987e5", "#1c5cab", "#104281"]));
const divRamp = computed(() => (dark.value ? ["#e66767", "#383835", "#3987e5"] : ["#e34948", "#f0efec", "#2a78d6"]));
const axis = computed(() => ({
  axisLine: { lineStyle: { color: vars.value.dividerColor } },
  axisTick: { show: false },
  axisLabel: { color: vars.value.textColor3, fontSize: 11 },
  splitLine: { lineStyle: { color: vars.value.dividerColor, opacity: 0.6 } },
}));
const tooltipBase = computed(() => ({
  backgroundColor: vars.value.popoverColor,
  borderColor: vars.value.dividerColor,
  textStyle: { color: vars.value.textColor1, fontSize: 12 },
}));

const kpis = computed(() => {
  const o = stats.value?.overall;
  if (!o) return [];
  return [
    { label: t("stats.contestants"), value: String(stats.value!.contestants), sub: t("stats.submitted") + " " + o.submitted },
    { label: t("stats.mean"), value: o.mean.toFixed(1), sub: `/ ${fmtScore(o.full)}` },
    { label: t("stats.median"), value: fmtScore(o.median), sub: `Q1 ${fmtScore(o.q1)} · Q3 ${fmtScore(o.q3)}` },
    { label: t("stats.std"), value: o.std_dev.toFixed(1), sub: "" },
    { label: t("stats.max"), value: fmtScore(o.max), sub: t("stats.fullCount") + " " + o.full_count },
    { label: t("stats.disc"), value: o.discrimination.toFixed(3), sub: "", hint: t("stats.discHint") },
  ];
});

const histOption = computed(() => {
  const s = stats.value;
  if (!s) return {};
  const b = s.histogram;
  return {
    grid: { left: 40, right: 12, top: 16, bottom: 36 },
    tooltip: {
      ...tooltipBase.value,
      trigger: "axis",
      axisPointer: { type: "shadow" },
      formatter: (ps: { dataIndex: number }[]) => {
        const x = b[ps[0].dataIndex];
        return `${fmtScore(x.lo)} – ${fmtScore(x.hi)}<br/><b>${x.count}</b> · ${(x.ratio * 100).toFixed(1)}%<br/>≥ ${fmtScore(x.lo)}: ${x.suffix}`;
      },
    },
    xAxis: { type: "category", data: b.map((x) => fmtScore(x.lo)), ...axis.value, splitLine: { show: false } },
    yAxis: { type: "value", minInterval: 1, ...axis.value },
    series: [{ type: "bar", data: b.map((x) => x.count), itemStyle: { color: blue.value, borderRadius: [4, 4, 0, 0] }, barCategoryGap: "12%" }],
  };
});

const boxOption = computed(() => {
  const s = stats.value;
  if (!s) return {};
  const pct = (v: number, full: number) => (full > 0 ? Math.round((v / full) * 1000) / 10 : 0);
  const data = s.problems.map((p) => {
    const m = p.summary;
    return [pct(m.min, m.full), pct(m.q1, m.full), pct(m.median, m.full), pct(m.q3, m.full), pct(m.max, m.full)];
  });
  return {
    grid: { left: 44, right: 12, top: 16, bottom: 36 },
    tooltip: {
      ...tooltipBase.value,
      trigger: "item",
      formatter: (p: { dataIndex: number }) => {
        const ps: ProblemStat = s.problems[p.dataIndex];
        const m = ps.summary;
        return `<b>${ps.title}</b><br/>max ${fmtScore(m.max)} · Q3 ${fmtScore(m.q3)}<br/>median ${fmtScore(m.median)} · mean ${m.mean.toFixed(1)}<br/>Q1 ${fmtScore(m.q1)} · min ${fmtScore(m.min)}`;
      },
    },
    xAxis: { type: "category", data: s.problems.map((p) => p.title), ...axis.value, splitLine: { show: false } },
    yAxis: { type: "value", min: 0, max: 100, name: "%", nameTextStyle: { color: vars.value.textColor3 }, ...axis.value },
    series: [{ type: "boxplot", data, itemStyle: { color: dark.value ? "rgba(57,135,229,.25)" : "rgba(42,120,214,.15)", borderColor: blue.value, borderWidth: 2 }, boxWidth: [12, 40] }],
  };
});

const heatOption = computed(() => {
  const s = stats.value;
  if (!s) return {};
  const maxSub = Math.max(1, ...s.problems.map((p) => p.subtasks.length));
  const data: [number, number, number][] = [];
  s.problems.forEach((p, y) => p.subtasks.forEach((st, x) => data.push([x, y, Math.round(st.pass_rate * 1000) / 10])));
  return {
    grid: { left: 90, right: 16, top: 8, bottom: 56 },
    tooltip: {
      ...tooltipBase.value,
      formatter: (p: { value: [number, number, number] }) => `${s.problems[p.value[1]].title} · #${p.value[0] + 1}<br/>${t("stats.passRate")} <b>${p.value[2]}%</b>`,
    },
    xAxis: { type: "category", data: Array.from({ length: maxSub }, (_, i) => `${i + 1}`), ...axis.value, splitLine: { show: false } },
    yAxis: { type: "category", data: s.problems.map((p) => p.title), inverse: true, ...axis.value, splitLine: { show: false } },
    visualMap: {
      min: 0, max: 100, calculable: false, orient: "horizontal", left: "center", bottom: 4, itemHeight: 120, itemWidth: 10,
      inRange: { color: seqRamp.value }, textStyle: { color: vars.value.textColor3, fontSize: 11 }, text: ["100%", "0%"],
    },
    series: [{
      type: "heatmap", data,
      label: { show: maxSub <= 25, fontSize: 10, color: "#fff", textBorderColor: "rgba(0,0,0,.45)", textBorderWidth: 2, formatter: (p: { value: number[] }) => `${Math.round(p.value[2])}` },
      itemStyle: { borderColor: vars.value.cardColor, borderWidth: 2, borderRadius: 3 },
      emphasis: { itemStyle: { borderColor: vars.value.textColor1 } },
    }],
  };
});

const corrOption = computed(() => {
  const s = stats.value;
  if (!s) return {};
  const names = s.problems.map((p) => p.title);
  const data: [number, number, number][] = [];
  s.correlation.forEach((row, i) => row.forEach((v, j) => data.push([j, i, Math.round(v * 100) / 100])));
  return {
    grid: { left: 90, right: 16, top: 8, bottom: 56 },
    tooltip: { ...tooltipBase.value, formatter: (p: { value: [number, number, number] }) => `${names[p.value[1]]} × ${names[p.value[0]]}<br/>r = <b>${p.value[2].toFixed(2)}</b>` },
    xAxis: { type: "category", data: names, ...axis.value, splitLine: { show: false } },
    yAxis: { type: "category", data: names, inverse: true, ...axis.value, splitLine: { show: false } },
    visualMap: {
      min: -1, max: 1, orient: "horizontal", left: "center", bottom: 4, itemHeight: 120, itemWidth: 10,
      inRange: { color: divRamp.value }, textStyle: { color: vars.value.textColor3, fontSize: 11 }, text: ["+1", "−1"],
    },
    series: [{
      type: "heatmap", data,
      label: { show: names.length <= 10, fontSize: 11, color: vars.value.textColor1, formatter: (p: { value: number[] }) => p.value[2].toFixed(2) },
      itemStyle: { borderColor: vars.value.cardColor, borderWidth: 2, borderRadius: 3 },
    }],
  };
});

const groupKeyOptions = computed(() => (stats.value?.group_keys ?? []).map((k) => ({ label: k, value: k })));
const groupRows = computed<GroupStat[]>(() => (stats.value?.groups ?? []).filter((g) => g.key === groupKey.value).sort((a, b) => b.summary.mean - a.summary.mean));
const groupOption = computed(() => {
  const g = groupRows.value;
  return {
    grid: { left: 90, right: 24, top: 8, bottom: 24 },
    tooltip: {
      ...tooltipBase.value, trigger: "axis", axisPointer: { type: "shadow" },
      formatter: (ps: { dataIndex: number }[]) => {
        const x = g[ps[0].dataIndex];
        return `<b>${x.value}</b> · ${t("stats.people", { n: x.summary.count })}<br/>${t("stats.mean")} ${x.summary.mean.toFixed(1)} · ${t("stats.median")} ${fmtScore(x.summary.median)}`;
      },
    },
    xAxis: { type: "value", ...axis.value },
    yAxis: { type: "category", data: g.map((x) => x.value), inverse: true, ...axis.value, splitLine: { show: false } },
    series: [{
      type: "bar", data: g.map((x) => Math.round(x.summary.mean * 10) / 10), itemStyle: { color: blue.value, borderRadius: [0, 4, 4, 0] },
      barMaxWidth: 22, label: { show: g.length <= 12, position: "right", color: vars.value.textColor2, fontSize: 11 },
    }],
  };
});

// ---- tables -----------------------------------------------------------------
const problemColumns = computed<DataTableColumns<ProblemStat>>(() => [
  { title: t("stats.problem"), key: "title", fixed: "left", width: 140, ellipsis: { tooltip: true } },
  { title: t("stats.full"), key: "full", align: "right", render: (r) => fmtScore(r.summary.full) },
  { title: t("stats.submitted"), key: "sub", align: "right", render: (r) => r.summary.submitted },
  { title: t("stats.mean"), key: "mean", align: "right", render: (r) => r.summary.mean.toFixed(2) },
  { title: t("stats.median"), key: "median", align: "right", render: (r) => fmtScore(r.summary.median) },
  { title: t("stats.std"), key: "std", align: "right", render: (r) => r.summary.std_dev.toFixed(2) },
  { title: t("stats.fullCount"), key: "fc", align: "right", render: (r) => r.summary.full_count },
  { title: t("stats.zeroCount"), key: "zc", align: "right", render: (r) => r.summary.zero_count },
  {
    title: () => h(NTooltip, null, { trigger: () => h("span", { style: "border-bottom:1px dotted" }, t("stats.disc")), default: () => t("stats.discHint") }),
    key: "disc", align: "right", render: (r) => r.summary.discrimination.toFixed(3),
  },
  { title: t("stats.avgTime"), key: "time", align: "right", render: (r) => (r.mean_time_ms ? `${Math.round(r.mean_time_ms)} ms` : "–") },
]);

const labelKeys = computed(() => {
  const order = ["AC", "PC", "WA", "TLE", "MLE", "OLE", "RE", "NO", "CE", "SE", "Missing"];
  const seen = new Set<string>();
  for (const p of stats.value?.problems ?? []) Object.keys(p.labels).forEach((k) => seen.add(k));
  return [...order.filter((k) => seen.has(k)), ...[...seen].filter((k) => !order.includes(k))];
});
const labelColumns = computed<DataTableColumns<ProblemStat>>(() => [
  { title: t("stats.problem"), key: "title", width: 140, ellipsis: { tooltip: true } },
  ...labelKeys.value.map((k) => ({
    title: t(`verdict.${k}`, k), key: k, align: "right" as const,
    render: (r: ProblemStat) => (r.labels[k] ? String(r.labels[k]) : h("span", { style: "opacity:.35" }, "0")),
  })),
]);

const groupColumns = computed<DataTableColumns<GroupStat>>(() => [
  { title: t("stats.group"), key: "value", width: 140, ellipsis: { tooltip: true } },
  { title: t("stats.count"), key: "count", align: "right", render: (r) => r.summary.count },
  { title: t("stats.mean"), key: "mean", align: "right", render: (r) => r.summary.mean.toFixed(1) },
  { title: t("stats.median"), key: "median", align: "right", render: (r) => fmtScore(r.summary.median) },
  { title: t("stats.max"), key: "max", align: "right", render: (r) => fmtScore(r.summary.max) },
  ...(stats.value?.problems ?? []).map((p, i) => ({
    title: p.title, key: `p${i}`, align: "right" as const, render: (r: GroupStat) => r.problem_means[i].toFixed(1),
  })),
]);

const countColumns = computed<DataTableColumns<ScoreCount>>(() => [
  { title: t("stats.score"), key: "score", align: "right", render: (r) => fmtScore(r.score) },
  { title: t("stats.count"), key: "count", align: "right" },
  { title: t("stats.ratio"), key: "ratio", align: "right", render: (r) => `${(r.ratio * 100).toFixed(1)}%` },
  { title: t("stats.atLeast"), key: "suffix", align: "right" },
  { title: t("stats.atMost"), key: "prefix", align: "right" },
]);

// ---- export -------------------------------------------------------------------
const exportOptions = computed(() => [
  { label: t("stats.exportHtml"), key: "html" },
  { label: t("stats.exportHtm"), key: "htm" },
  { label: t("stats.exportXlsx"), key: "xlsx" },
  { label: t("stats.exportCsv"), key: "csv" },
]);
async function doExport(fmt: string) {
  const base = (state.contest?.title || "results").replace(/[\\/:*?"<>|]/g, "_");
  const path = await save({ defaultPath: `${state.root}\\${base}.${fmt}`, filters: [{ name: fmt.toUpperCase(), extensions: [fmt] }] });
  if (!path) return;
  try {
    await api.exportResults(fmt, path);
    message.success(t("stats.exported", { p: path }));
  } catch (e) {
    message.error(String(e));
  }
}
</script>

<template>
  <n-scrollbar style="height: 100%">
    <n-spin :show="loading && !stats">
      <div class="page">
        <div class="bar">
          <n-button size="small" :loading="loading" @click="load">{{ t("stats.refresh") }}</n-button>
          <n-dropdown :options="exportOptions" @select="doExport">
            <n-button size="small" type="primary" secondary>{{ t("stats.export") }} ▾</n-button>
          </n-dropdown>
        </div>
        <n-empty v-if="stats && !stats.contestants" :description="t('stats.empty')" style="margin-top: 80px" />
        <template v-else-if="stats">
          <div class="kpis">
            <div v-for="k in kpis" :key="k.label" class="kpi" :title="k.hint">
              <div class="kl">{{ k.label }}</div>
              <div class="kv tabular">{{ k.value }}</div>
              <div class="ks">{{ k.sub }}</div>
            </div>
          </div>

          <div class="grid2">
            <n-card size="small" :title="t('stats.histogram')">
              <template #header-extra><span class="hint">{{ t("stats.histogramHint") }}</span></template>
              <v-chart :option="histOption" autoresize style="height: 260px" />
            </n-card>
            <n-card size="small" :title="t('stats.box')">
              <template #header-extra><span class="hint">{{ t("stats.boxHint") }}</span></template>
              <v-chart :option="boxOption" autoresize style="height: 260px" />
            </n-card>
          </div>

          <n-card size="small" :title="t('stats.problemTable')" class="block">
            <n-data-table :columns="problemColumns" :data="stats.problems" size="small" :bordered="false" :scroll-x="900" />
          </n-card>

          <div class="grid2">
            <n-card size="small" :title="t('stats.heat')">
              <template #header-extra><span class="hint">{{ t("stats.heatHint") }}</span></template>
              <v-chart :option="heatOption" autoresize :style="{ height: `${Math.max(180, stats.problems.length * 36 + 80)}px` }" />
            </n-card>
            <n-card size="small" :title="t('stats.corr')">
              <template #header-extra><span class="hint">{{ t("stats.corrHint") }}</span></template>
              <v-chart :option="corrOption" autoresize :style="{ height: `${Math.max(180, stats.problems.length * 36 + 80)}px` }" />
            </n-card>
          </div>

          <n-card size="small" :title="t('stats.labels')" class="block">
            <n-data-table :columns="labelColumns" :data="stats.problems" size="small" :bordered="false" :scroll-x="700" />
          </n-card>

          <n-card size="small" :title="t('stats.groups')" class="block">
            <div v-if="!stats.group_keys.length" class="hint">{{ t("stats.groupHint") }}</div>
            <template v-else>
              <div style="display: flex; gap: 8px; align-items: center; margin-bottom: 8px">
                <span class="hint">{{ t("stats.groupKey") }}</span>
                <n-select v-model:value="groupKey" :options="groupKeyOptions" size="small" style="width: 200px" />
              </div>
              <v-chart :option="groupOption" autoresize :style="{ height: `${Math.max(120, groupRows.length * 30 + 40)}px` }" />
              <n-data-table :columns="groupColumns" :data="groupRows" size="small" :bordered="false" :scroll-x="800" style="margin-top: 8px" />
            </template>
          </n-card>

          <n-collapse class="block">
            <n-collapse-item :title="t('stats.distribution')" name="d">
              <n-data-table :columns="countColumns" :data="stats.counts" size="small" :max-height="400" virtual-scroll />
            </n-collapse-item>
          </n-collapse>
        </template>
      </div>
    </n-spin>
  </n-scrollbar>
</template>

<style scoped>
.page {
  padding: 12px 16px 24px;
  max-width: 1400px;
}
.bar {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
  margin-bottom: 12px;
}
.kpis {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
  gap: 12px;
  margin-bottom: 12px;
}
.kpi {
  border: 1px solid rgba(127, 127, 127, 0.2);
  border-radius: 8px;
  padding: 10px 14px;
}
.kl {
  font-size: 12px;
  opacity: 0.65;
}
.kv {
  font-size: 24px;
  font-weight: 600;
  line-height: 1.3;
}
.ks {
  font-size: 11px;
  opacity: 0.55;
  min-height: 15px;
}
.grid2 {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(420px, 1fr));
  gap: 12px;
  margin-bottom: 12px;
}
.block {
  margin-bottom: 12px;
}
.hint {
  font-size: 12px;
  opacity: 0.6;
}
</style>
