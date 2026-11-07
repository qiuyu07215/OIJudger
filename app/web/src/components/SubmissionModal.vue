<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { NModal, NCard, NSpin } from "naive-ui";
import { useI18n } from "vue-i18n";
import { api, type SubmissionDetail } from "../api";
import { onNotice, state } from "../store";
import { clock, fmtScore } from "../util";
import PBadge from "./PBadge.vue";
import Result from "./Result.vue";

const props = defineProps<{ id: number | null }>();
const emit = defineEmits<{ close: [] }>();
const { t, te } = useI18n();
const d = ref<SubmissionDetail | null>(null);

async function load() {
  if (props.id === null) return;
  d.value = await api.submission(props.id).catch(() => null);
}
watch(() => props.id, (id) => {
  d.value = null;
  if (id !== null) load();
});
const off = onNotice((n) => {
  if (n.type === "submission" && n.id === props.id) load();
});
onBeforeUnmount(off);
const ioi = computed(() => state.info?.format === "ioi");
const vt = (v: string) => (te(`result.${v}`) ? t(`result.${v}`) : v);
const caseCls = (v: string) => (v === "AC" ? "correct" : v === "Skipped" ? "queued" : "incorrect");
</script>

<template>
  <n-modal :show="id !== null" @update:show="(v: boolean) => !v && emit('close')">
    <n-card :title="t('sub.title', { id: id ?? '' })" style="width: min(760px, 95vw)" :bordered="false" size="small" closable @close="emit('close')">
      <n-spin :show="!d">
        <template v-if="d">
          <table class="dj info">
            <tr><th>{{ t("sub.problem") }}</th><td><PBadge :problem="d.problem" /> {{ state.info?.problems.find((p) => p.name === d!.problem)?.title }}</td></tr>
            <tr><th>{{ t("sub.lang") }}</th><td class="langid">{{ d.lang === "c" ? "C" : "CPP" }}</td></tr>
            <tr><th>{{ t("sub.time") }}</th><td class="tabular">{{ clock(d.at_ms) }}</td></tr>
            <tr><th>{{ t("sub.result") }}</th><td><Result :s="d" /><span v-if="ioi && d.score !== null && d.state === 'done'" class="tabular"> · {{ fmtScore(d.score) }} / {{ fmtScore(d.full) }}</span></td></tr>
          </table>
          <template v-if="d.message">
            <h4>{{ t("sub.compile") }}</h4>
            <pre class="code">{{ d.message }}</pre>
          </template>
          <template v-if="d.subtasks.length">
            <h4>{{ t("sub.cases") }}</h4>
            <div v-for="(st, si) in d.subtasks" :key="si" class="st">
              <div class="st-head"><b>{{ t("sub.subtask", { n: si + 1 }) }}</b> <span class="tabular">{{ fmtScore(st.score) }} / {{ fmtScore(st.full) }}</span></div>
              <span v-for="(c, ci) in st.cases" :key="ci" class="case" :class="caseCls(c.verdict)" :title="`#${ci + 1} ${vt(c.verdict)} · ${c.time_ms} ms · ${(c.memory_kb / 1024).toFixed(1)} MiB`">{{ ci + 1 }}</span>
            </div>
          </template>
          <h4>{{ t("sub.source") }}</h4>
          <pre class="code src">{{ d.source }}</pre>
        </template>
      </n-spin>
    </n-card>
  </n-modal>
</template>

<style scoped>
table.info th {
  width: 120px;
  border-bottom: 1px solid var(--divider);
}
h4 {
  margin: 16px 0 6px;
  font-size: 14px;
}
.st {
  margin-bottom: 8px;
}
.st-head {
  font-size: 13px;
  margin-bottom: 4px;
}
.case {
  display: inline-block;
  min-width: 22px;
  padding: 2px 4px;
  margin: 0 3px 3px 0;
  border-radius: 3px;
  text-align: center;
  font-size: 11px;
  color: #fff;
  cursor: default;
}
.case.correct {
  background: var(--sb-first);
}
.case.incorrect {
  background: #c0533c;
}
.case.queued {
  background: var(--text3);
}
.src {
  max-height: 340px;
}
</style>
