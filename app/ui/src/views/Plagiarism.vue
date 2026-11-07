<script setup lang="ts">
import { computed, h, ref } from "vue";
import { NButton, NDataTable, NEmpty, NModal, NCard, NSelect, NSlider, NSpin, NProgress, useMessage, type DataTableColumns } from "naive-ui";
import { useI18n } from "vue-i18n";
import { api, type SimPair, type SourceFile } from "../api";
import { state } from "../store";

const { t } = useI18n();
const message = useMessage();
const problem = ref<string | null>(state.contest?.problem.find((p) => p.kind !== "answer")?.name ?? null);
const threshold = ref(70);
const pairs = ref<SimPair[] | null>(null);
const running = ref(false);
const viewing = ref<SimPair | null>(null);
const sources = ref<[SourceFile | undefined, SourceFile | undefined]>([undefined, undefined]);

const problemOptions = computed(() =>
  (state.contest?.problem ?? []).filter((p) => p.kind !== "answer").map((p) => ({ label: p.title || p.name, value: p.name })),
);

async function run() {
  if (!problem.value) return;
  running.value = true;
  try {
    pairs.value = await api.similarity(problem.value, threshold.value / 100);
  } catch (e) {
    message.error(String(e));
  } finally {
    running.value = false;
  }
}

async function open(p: SimPair) {
  viewing.value = p;
  sources.value = [undefined, undefined];
  const [a, b] = await Promise.all([api.readSource(p.a, problem.value!), api.readSource(p.b, problem.value!)]);
  sources.value = [a[0], b[0]];
}

function level(x: number) {
  return x >= 0.9 ? "error" : x >= 0.75 ? "warning" : "info";
}

const columns = computed<DataTableColumns<SimPair>>(() => [
  {
    title: t("sim.similarity"), key: "similarity", width: 180, sorter: (a, b) => a.similarity - b.similarity,
    render: (r) => h(NProgress, { type: "line", percentage: Math.round(r.similarity * 1000) / 10, status: level(r.similarity), indicatorPlacement: "inside", height: 16 }),
  },
  { title: "A", key: "a", ellipsis: { tooltip: true } },
  { title: "B", key: "b", ellipsis: { tooltip: true } },
  { title: t("sim.shared"), key: "shared", width: 110, align: "right" },
  { title: t("sim.jaccard"), key: "jaccard", width: 100, align: "right", render: (r) => `${(r.jaccard * 100).toFixed(0)}%` },
  { title: "", key: "op", width: 110, render: (r) => h(NButton, { size: "tiny", onClick: () => open(r) }, () => t("sim.compare")) },
]);

function lines(src: SourceFile | undefined, ranges: [number, number][]) {
  const text = src?.text ?? "";
  return text.split(/\r?\n/).map((l, i) => ({ n: i + 1, l, hit: ranges.some(([a, b]) => i + 1 >= a && i + 1 <= b) }));
}
</script>

<template>
  <div class="page">
    <div class="bar">
      <span class="hint">{{ t("sim.problem") }}</span>
      <n-select v-model:value="problem" :options="problemOptions" size="small" style="width: 220px" />
      <span class="hint" style="margin-left: 12px">{{ t("sim.threshold") }} {{ threshold }}%</span>
      <n-slider v-model:value="threshold" :min="30" :max="100" :step="5" style="width: 180px" />
      <n-button type="primary" size="small" :loading="running" :disabled="!problem" @click="run">{{ t("sim.run") }}</n-button>
    </div>
    <div class="hint" style="margin-bottom: 10px">{{ t("sim.hint") }}</div>
    <n-spin :show="running" style="flex: 1; min-height: 0; display: flex; flex-direction: column">
      <n-empty v-if="pairs && !pairs.length" :description="t('sim.none')" style="margin-top: 60px" />
      <template v-else-if="pairs">
        <div class="hint" style="margin-bottom: 6px">{{ t("sim.pairs", { n: pairs.length }) }}</div>
        <n-data-table :columns="columns" :data="pairs" size="small" virtual-scroll :max-height="600" :row-key="(r: SimPair) => r.a + '|' + r.b" />
      </template>
    </n-spin>

    <n-modal :show="!!viewing" @update:show="(v: boolean) => !v && (viewing = null)">
      <n-card v-if="viewing" style="width: min(1400px, 96vw)" closable :title="`${viewing.a} ↔ ${viewing.b} · ${(viewing.similarity * 100).toFixed(1)}%`" @close="viewing = null">
        <div class="hint" style="margin-bottom: 8px">{{ t("sim.matched") }}</div>
        <div class="split">
          <div v-for="(side, k) in [{ name: viewing.a, src: sources[0], r: viewing.a_lines }, { name: viewing.b, src: sources[1], r: viewing.b_lines }]" :key="k" class="side">
            <div class="mono hint" style="margin-bottom: 4px">{{ side.src?.path ?? side.name }}</div>
            <div class="code-view mono">
              <div v-for="ln in lines(side.src, side.r)" :key="ln.n" class="ln" :class="{ hit: ln.hit }"><span class="no">{{ ln.n }}</span>{{ ln.l }}</div>
            </div>
          </div>
        </div>
      </n-card>
    </n-modal>
  </div>
</template>

<style scoped>
.page {
  height: 100%;
  box-sizing: border-box;
  padding: 12px 16px;
  display: flex;
  flex-direction: column;
}
.bar {
  display: flex;
  gap: 8px;
  align-items: center;
  flex-wrap: wrap;
  margin-bottom: 8px;
}
.hint {
  font-size: 12px;
  opacity: 0.65;
}
.split {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}
.side {
  min-width: 0;
}
.code-view {
  font-size: 12px;
  line-height: 1.55;
  max-height: 70vh;
  overflow: auto;
  border-radius: 6px;
  background: rgba(127, 127, 127, 0.07);
  padding: 6px 0;
  white-space: pre;
}
.ln {
  padding: 0 10px 0 0;
}
.ln.hit {
  background: rgba(230, 149, 0, 0.22);
}
.no {
  display: inline-block;
  width: 40px;
  text-align: right;
  margin-right: 10px;
  opacity: 0.4;
  user-select: none;
}
</style>
