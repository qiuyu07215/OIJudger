<script setup lang="ts">
import { computed } from "vue";
import { NProgress, NTag, NSpace, NEllipsis } from "naive-ui";
import { useI18n } from "vue-i18n";
import { state } from "../store";

const { t } = useI18n();
const p = computed(() => state.progress);
const pct = computed(() => (p.value && p.value.total ? Math.round((p.value.done / p.value.total) * 1000) / 10 : 0));

function fmtTime(ms: number) {
  const s = Math.round(ms / 1000);
  return s >= 60 ? `${Math.floor(s / 60)}m${String(s % 60).padStart(2, "0")}s` : `${s}s`;
}
const eta = computed(() => {
  const v = p.value;
  if (!v || !v.done || v.done >= v.total) return "";
  return fmtTime((v.elapsed_ms / v.done) * (v.total - v.done));
});
const countTags = computed(() => Object.entries(p.value?.counts ?? {}).sort((a, b) => b[1] - a[1]));
const tagType = (l: string) => (l === "AC" ? "success" : l === "PC" ? "warning" : l === "Missing" ? "default" : "error");
</script>

<template>
  <div v-if="p && (p.running || p.total)" class="panel">
    <div style="display: flex; align-items: center; gap: 16px">
      <div style="font-weight: 600; white-space: nowrap">
        {{ p.running ? t("progress.title") : p.done < p.total ? t("progress.cancelled") : t("progress.finished") }}
      </div>
      <n-progress type="line" :percentage="pct" :processing="p.running" :height="10" style="flex: 1" :show-indicator="false" />
      <div class="tabular" style="white-space: nowrap; font-size: 13px">
        {{ t("progress.done", { done: p.done, total: p.total }) }} · {{ t("progress.elapsed", { t: fmtTime(p.elapsed_ms) }) }}
        <template v-if="p.running && eta"> · {{ t("progress.eta", { t: eta }) }}</template>
      </div>
    </div>
    <div style="display: flex; gap: 16px; margin-top: 6px; align-items: center; min-height: 24px">
      <n-space :size="4">
        <n-tag v-for="[k, n] in countTags" :key="k" size="small" :type="tagType(k)" :bordered="false">
          {{ t(`verdict.${k}`, k) }} {{ n }}
        </n-tag>
      </n-space>
      <div v-if="p.running" style="flex: 1; display: flex; gap: 6px; overflow: hidden; justify-content: flex-end">
        <span v-for="(w, i) in p.workers" :key="i" class="worker mono">
          <n-ellipsis style="max-width: 150px">{{ w ? `${w.contestant}/${w.problem}${w.cases_done ? ` #${w.cases_done}` : ""}` : t("progress.idle") }}</n-ellipsis>
        </span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.panel {
  border-top: 1px solid rgba(127, 127, 127, 0.2);
  padding: 8px 16px 10px;
}
.worker {
  font-size: 11px;
  padding: 1px 6px;
  border-radius: 4px;
  background: rgba(127, 127, 127, 0.1);
  white-space: nowrap;
}
</style>
