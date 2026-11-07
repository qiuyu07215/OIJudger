<script setup lang="ts">
import { computed, h, ref } from "vue";
import { NDataTable, NInput, NEmpty, NTooltip, type DataTableColumns } from "naive-ui";
import { useI18n } from "vue-i18n";
import { state, rows, cells, getCell, type Row } from "../store";
import { fmtScore, fullScore } from "../api";
import ResultDrawer from "./ResultDrawer.vue";

const selected = defineModel<string[]>("selected", { default: () => [] });
const { t } = useI18n();
const filter = ref("");
const detail = ref<{ contestant: string; problem: string } | null>(null);

const filtered = computed(() => {
  const f = filter.value.trim().toLowerCase();
  return f ? rows.value.filter((r) => r.contestant.toLowerCase().includes(f)) : rows.value;
});

function cellStyle(score: number, full: number, status: string, label: string) {
  if (status === "NoSource") return { background: "var(--cell-missing)", color: "var(--cell-missing-fg)" };
  if (status === "CompileError") return { background: "var(--cell-ce)", color: "var(--cell-ce-fg)" };
  if (status === "SystemError") return { background: "var(--cell-se)", color: "var(--cell-se-fg)" };
  if (full > 0 && score >= full - 1e-9) return { background: "var(--cell-full)", color: "var(--cell-full-fg)" };
  if (score <= 1e-9) return { background: "var(--cell-zero)", color: "var(--cell-zero-fg)" };
  const r = Math.max(0, Math.min(1, score / full));
  // Partial scores stay in the red → amber range so they never read as AC.
  const hue = Math.round(12 + r * 36);
  return { background: `hsla(${hue}, 70%, 50%, var(--partial-a))`, color: `hsl(${hue}, 60%, var(--partial-l))` };
}

const columns = computed<DataTableColumns<Row>>(() => {
  void cells.value; // re-render cells on updates
  const probs = state.contest?.problem ?? [];
  const cols: DataTableColumns<Row> = [
    { type: "selection", fixed: "left", width: 40 },
    { title: t("results.rank"), key: "rank", width: 64, fixed: "left", align: "right", className: "tabular" },
    {
      title: t("results.contestant"),
      key: "contestant",
      width: 160,
      fixed: "left",
      ellipsis: { tooltip: true },
      render: (r) =>
        h("a", { class: "name-link", onClick: () => (detail.value = { contestant: r.contestant, problem: "" }) }, r.contestant),
    },
    {
      title: t("results.total"),
      key: "total",
      width: 84,
      align: "right",
      fixed: "left",
      sorter: (a, b) => a.total - b.total,
      render: (r) => h("b", { class: "tabular" }, fmtScore(r.total)),
    },
  ];
  for (const p of probs) {
    const full = fullScore(p);
    cols.push({
      title: () =>
        h(NTooltip, null, {
          trigger: () => h("span", p.title || p.name),
          default: () => `${p.name} · ${fmtScore(full)}`,
        }),
      key: `p:${p.name}`,
      width: Math.max(96, Math.min(160, (p.title || p.name).length * 12 + 40)),
      align: "center",
      sorter: (a, b) => (getCell(a.contestant, p.name)?.score ?? -1) - (getCell(b.contestant, p.name)?.score ?? -1),
      render: (r) => {
        const c = getCell(r.contestant, p.name);
        const open = () => (detail.value = { contestant: r.contestant, problem: p.name });
        if (!c) return h("div", { class: "score-cell", style: { color: "var(--cell-missing-fg)" }, onClick: open }, "·");
        const showLabel = c.status !== "Judged" ? t(`verdict.${c.label}`, c.label) : c.score < c.full ? c.label : "";
        const text = c.status === "Judged" ? fmtScore(c.score) : "";
        return h(
          "div",
          {
            class: ["score-cell", c.stale ? "stale" : ""],
            style: cellStyle(c.score, c.full, c.status, c.label),
            title: c.stale ? t("results.stale") : undefined,
            onClick: open,
          },
          [text ? h("span", text) : null, showLabel ? h("span", { class: "label" }, showLabel) : null],
        );
      },
    });
  }
  cols.push({
    title: t("results.judgedAt"),
    key: "judged",
    width: 150,
    render: (r) => {
      let latest = 0;
      for (const p of probs) latest = Math.max(latest, getCell(r.contestant, p.name)?.judged_at ?? 0);
      return h("span", { style: "color: var(--text3); font-size: 12px" }, latest ? new Date(latest * 1000).toLocaleString() : "");
    },
  });
  cols.push({
    title: t("results.time"),
    key: "time",
    width: 90,
    align: "right",
    render: (r) => h("span", { class: "tabular", style: "opacity:.7" }, `${(r.time / 1000).toFixed(2)}s`),
  });
  return cols;
});
</script>

<template>
  <div style="height: 100%; display: flex; flex-direction: column; padding: 8px 16px 0; box-sizing: border-box">
    <div style="display: flex; gap: 12px; align-items: center; margin-bottom: 8px">
      <n-input v-model:value="filter" :placeholder="t('results.filter')" clearable size="small" style="max-width: 260px" />
      <span style="font-size: 12px; opacity: 0.6">
        {{ t("results.contestants", { n: state.contestants.length }) }} ·
        {{ t("results.problems", { n: state.contest?.problem.length ?? 0 }) }}
      </span>
    </div>
    <n-empty v-if="!state.contestants.length" :description="t('results.noContestants')" style="margin-top: 80px" />
    <n-data-table
      v-else
      v-model:checked-row-keys="selected"
      :columns="columns"
      :data="filtered"
      :row-key="(r: Row) => r.contestant"
      size="small"
      virtual-scroll
      flex-height
      :scroll-x="900"
      style="flex: 1; min-height: 0"
    />
    <ResultDrawer :target="detail" @close="detail = null" />
  </div>
</template>

<style scoped>
:deep(.name-link) {
  color: var(--text);
  cursor: pointer;
  text-decoration: underline dotted color-mix(in srgb, var(--text3) 60%, transparent);
  text-underline-offset: 3px;
}
:deep(.name-link:hover) {
  color: var(--accent);
}
</style>
