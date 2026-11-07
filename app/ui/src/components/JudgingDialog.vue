<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { NModal, NCard, NProgress, NButton, NTag, NSpace, NCheckbox, NEllipsis } from "naive-ui";
import { useI18n } from "vue-i18n";
import { api, fmtScore, type LogLine } from "../api";
import { state, judging, log } from "../store";

const { t } = useI18n();
const p = computed(() => state.progress);
/** Two-phase mode: the compile phase runs before judging starts. */
const compiling = computed(() => !!p.value && p.value.running && !p.value.total && p.value.compile_total > 0);
/** Overlapped mode: still compiling while judging. */
const alsoCompiling = computed(
  () => !!p.value && p.value.running && !!p.value.total && p.value.compile_done < p.value.compile_total,
);
const pct = computed(() => {
  const v = p.value;
  if (!v) return 0;
  if (compiling.value) return Math.round((v.compile_done / v.compile_total) * 1000) / 10;
  return v.total ? Math.round((v.done / v.total) * 1000) / 10 : 0;
});
const follow = ref(true);
const box = ref<HTMLElement | null>(null);

function fmtTime(ms: number) {
  const s = Math.round(ms / 1000);
  return s >= 60 ? `${Math.floor(s / 60)}m${String(s % 60).padStart(2, "0")}s` : `${s}s`;
}
const eta = computed(() => {
  const v = p.value;
  if (!v || !v.done || v.done >= v.total) return "";
  return fmtTime((v.elapsed_ms / v.done) * (v.total - v.done));
});
const counts = computed(() => Object.entries(p.value?.counts ?? {}).sort((a, b) => b[1] - a[1]));
const tagType = (l: string) => (l === "AC" ? "success" : l === "PC" ? "warning" : l === "Missing" ? "default" : "error");

// Only render the tail: the log can hold thousands of lines.
const visible = computed(() => log.value.slice(-600));
watch(log, async () => {
  if (!follow.value) return;
  await nextTick();
  if (box.value) box.value.scrollTop = box.value.scrollHeight;
});

function cls(l: LogLine) {
  if (l.subtask === 0) return l.verdict === "AC" ? "ok sum" : "sum";
  return { AC: "ok", PC: "pc", SKIP: "skip", NO: "bad", SE: "se" }[l.verdict] ?? "bad";
}
function verdictText(v: string) {
  return t(`verdict.${v}`, v);
}
</script>

<template>
  <n-modal :show="state.showJudging" :mask-closable="!judging" @update:show="(v: boolean) => (state.showJudging = v)">
    <n-card :title="t('judging.title')" style="width: min(900px, 94vw)" :bordered="false" size="small" role="dialog">
      <template v-if="p">
        <div class="head">
          <n-progress type="line" :percentage="pct" :processing="judging" :height="12" :show-indicator="false" color="#D97757" style="flex: 1" />
          <span class="tabular nums">
            {{ compiling ? t("judging.compiling", { done: p.compile_done, total: p.compile_total }) : t("progress.done", { done: p.done, total: p.total }) }}
            <template v-if="alsoCompiling"> · {{ t("judging.compiledShort", { done: p.compile_done, total: p.compile_total }) }}</template>
            <span v-if="judging && p.compile_paused" class="paused" :title="t('judging.pausedHint')"> · {{ t("judging.paused") }}</span>
          </span>
        </div>
        <div class="sub">
          {{ t("progress.elapsed", { t: fmtTime(p.elapsed_ms) }) }}
          <template v-if="judging && eta"> · {{ t("progress.eta", { t: eta }) }}</template>
        </div>
        <n-space :size="4" style="margin: 8px 0">
          <n-tag v-for="[k, n] in counts" :key="k" size="small" :type="tagType(k)" :bordered="false">{{ verdictText(k) }} {{ n }}</n-tag>
        </n-space>
        <div v-if="judging" class="workers">
          <span v-for="(w, i) in p.workers" :key="i" class="worker mono">
            <n-ellipsis style="max-width: 200px">#{{ i + 1 }} {{ w ? `${w.contestant} / ${w.problem}${w.cases_done ? ` · ${w.cases_done}` : ""}` : t("progress.idle") }}</n-ellipsis>
          </span>
        </div>
        <div ref="box" class="log mono">
          <div v-if="!visible.length" class="empty">{{ t("judging.empty") }}</div>
          <div v-for="(l, i) in visible" :key="i" class="line" :class="cls(l)">
            <template v-if="l.subtask === 0">
              <b>{{ l.contestant }}</b> / {{ l.problem }} — {{ t("judging.total") }} {{ l.score ? `${fmtScore(l.score[0])} / ${fmtScore(l.score[1])}` : "" }}
              <span class="verdict">{{ verdictText(l.verdict) }}</span>
              <span v-if="l.message" class="msg">{{ l.message }}</span>
            </template>
            <template v-else>
              <span class="who">{{ l.contestant }} / {{ l.problem }}</span>
              <span class="pt">{{ t("judging.point", { n: l.subtask }) }}</span>
              <span class="verdict">{{ verdictText(l.verdict) }}</span>
              <span v-if="l.verdict !== 'SKIP'" class="tm">{{ l.time_ms }} ms · {{ (l.memory_kb / 1024).toFixed(1) }} MiB</span>
              <span v-if="l.message" class="msg">{{ l.message }}</span>
            </template>
          </div>
        </div>
      </template>
      <template #footer>
        <div style="display: flex; align-items: center; gap: 8px">
          <n-checkbox v-model:checked="follow" size="small">{{ t("judging.follow") }}</n-checkbox>
          <span style="flex: 1" />
          <template v-if="judging">
            <n-button size="small" @click="state.showJudging = false">{{ t("judging.background") }}</n-button>
            <n-button size="small" type="error" @click="api.cancelJudge()">{{ t("judging.stop") }}</n-button>
          </template>
          <n-button v-else size="small" type="primary" @click="state.showJudging = false">{{ t("judging.close") }}</n-button>
        </div>
      </template>
    </n-card>
  </n-modal>
</template>

<style scoped>
.head {
  display: flex;
  align-items: center;
  gap: 12px;
}
.nums {
  font-size: 13px;
  color: var(--text2);
  white-space: nowrap;
}
.sub {
  font-size: 12px;
  color: var(--text3);
  margin-top: 4px;
}
.workers {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-bottom: 8px;
}
.worker {
  font-size: 11px;
  padding: 2px 8px;
  border-radius: 6px;
  background: var(--panel);
  color: var(--text2);
}
.log {
  height: 340px;
  overflow: auto;
  border: 1px solid var(--divider);
  border-radius: 8px;
  background: var(--panel);
  padding: 6px 0;
  font-size: 12px;
  line-height: 1.6;
}
.empty {
  color: var(--text3);
  padding: 0 12px;
}
.line {
  padding: 0 12px;
  white-space: nowrap;
  display: flex;
  gap: 10px;
}
.line .who {
  color: var(--text2);
  min-width: 180px;
}
.line .pt {
  color: var(--text3);
  min-width: 70px;
}
.line .verdict {
  font-weight: 600;
  min-width: 70px;
}
.line .tm {
  color: var(--text3);
}
.line .msg {
  color: var(--text3);
  overflow: hidden;
  text-overflow: ellipsis;
}
.line.ok .verdict {
  color: var(--cell-full-fg);
}
.line.pc .verdict {
  color: #b7791f;
}
.line.bad .verdict {
  color: var(--cell-zero-fg);
}
.line.se .verdict {
  color: var(--cell-se-fg);
}
.line.skip {
  opacity: 0.55;
}
.line.sum {
  border-top: 1px dashed var(--divider);
  margin: 2px 0 4px;
  padding-top: 2px;
}
.paused {
  color: var(--accent);
}
</style>
