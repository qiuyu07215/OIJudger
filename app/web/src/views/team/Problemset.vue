<script setup lang="ts">
// DOMjudge problemset: one card per problem with label, name, limits, a
// submission/acceptance strip over the contest (hover for numbers),
// Submit, statement and samples.
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { NButton, NPopover, NEmpty, NIcon } from "naive-ui";
import { CloudUpload, FileText, Download, MessageCircle } from "@vicons/tabler";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import { api, type Clarification } from "../../api";
import { onNotice, openSubmit, state, throttle } from "../../store";
import { contrastText, fmtScore } from "../../util";

const { t } = useI18n();
const router = useRouter();
const stats = ref<Record<string, [number, number][]>>({});
const clars = ref<Clarification[]>([]);
const load = throttle(async () => {
  const s = await api.problemsetStats().catch(() => []);
  stats.value = Object.fromEntries(s.map((x) => [x.problem, x.intervals]));
  clars.value = await api.clarifications().catch(() => []);
}, 2000);
const off = onNotice((n) => {
  if (n.type === "scoreboard" || n.type === "clarification") load();
});
onMounted(load);
onBeforeUnmount(off);

const problems = computed(() => state.info?.problems ?? []);
const clarCount = (p: string) => clars.value.filter((c) => c.problem === p && c.answer).length;
const maxCount = computed(() => {
  let m = 1;
  for (const v of Object.values(stats.value)) for (const [a, r] of v) m = Math.max(m, a, r);
  return m;
});
function box(n: number, good: boolean) {
  if (!n) return {};
  const a = 0.25 + 0.75 * (n / maxCount.value);
  return { background: good ? `rgba(79, 122, 58, ${a})` : `rgba(181, 67, 46, ${a})` };
}
const intervalMin = computed(() => Math.round(((state.info?.end_ms ?? 0) - (state.info?.start_ms ?? 0)) / 60000 / 18));
</script>

<template>
  <div class="dj-container">
    <h1 class="title">{{ t("problemset.title", { c: state.info?.title ?? "" }) }}</h1>
    <n-empty v-if="!problems.length" :description="t('problemset.notStarted')" style="margin-top: 40px" />
    <div class="cards">
      <div v-for="p in problems" :key="p.name" class="dj-card card">
        <span class="badge" :style="p.color ? { background: p.color, color: contrastText(p.color) } : {}">{{ p.letter }}</span>
        <div class="name">{{ p.title }}</div>
        <div class="muted small">{{ t("problemset.limits", { t: p.time_ms / 1000, m: p.memory_mb }) }}</div>
        <div v-if="state.info?.format !== 'icpc'" class="muted small">{{ t("problemset.full", { n: fmtScore(p.full) }) }}</div>
        <n-popover v-if="stats[p.name]" trigger="hover">
          <template #trigger>
            <div class="strip">
              <div class="line">
                <span v-for="(x, i) in stats[p.name]" :key="i" class="b" :style="box(x[0], true)" />
              </div>
              <div class="line">
                <span v-for="(x, i) in stats[p.name]" :key="i" class="b" :style="box(x[1], false)" />
              </div>
            </div>
          </template>
          <div class="small">{{ t("problemset.stats") }} ({{ intervalMin }} min)</div>
          <div class="small tabular">{{ stats[p.name].map((x) => `${x[0]}/${x[1]}`).join("  ") }}</div>
        </n-popover>
        <div class="btns">
          <n-button v-if="state.me?.role === 'contestant'" type="success" size="small" @click="openSubmit(p.name)">
            <template #icon><n-icon :component="CloudUpload" /></template>{{ t("problemset.submit") }}
          </n-button>
          <n-button v-if="clarCount(p.name)" size="small" secondary @click="router.push('/team')">
            <template #icon><n-icon :component="MessageCircle" /></template>{{ t("problemset.clarifications", { n: clarCount(p.name) }) }}
          </n-button>
        </div>
        <div class="btns">
          <n-button v-if="p.statement" size="small" secondary @click="router.push(`/team/problems/${p.name}`)">
            <template #icon><n-icon :component="FileText" /></template>{{ t("problemset.statement") }}
          </n-button>
          <n-popover v-if="p.samples.length" trigger="click">
            <template #trigger>
              <n-button size="small" secondary>
                <template #icon><n-icon :component="Download" /></template>{{ t("problemset.samples") }}
              </n-button>
            </template>
            <div class="samples">
              <a v-for="f in p.samples" :key="f" :href="`/api/problems/${encodeURIComponent(p.name)}/samples/${encodeURIComponent(f)}`" download class="mono">{{ f }}</a>
            </div>
          </n-popover>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.title {
  text-align: center;
  font-weight: 500;
  font-size: 26px;
  margin: 6px 0 18px;
}
.cards {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
  gap: 14px;
  max-width: 1100px;
  margin: 0 auto;
}
.card {
  padding: 16px 14px;
  text-align: center;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
}
.badge {
  display: inline-block;
  min-width: 30px;
  padding: 3px 6px;
  border-radius: 4px;
  font-weight: 700;
  background: var(--accent-soft);
  border: 1px solid rgba(0, 0, 0, 0.2);
  box-sizing: border-box;
}
.name {
  font-size: 16px;
  margin-top: 4px;
}
.small {
  font-size: 12.5px;
}
.strip {
  margin: 8px 0 4px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  cursor: default;
}
.line {
  display: flex;
  gap: 2px;
}
.b {
  width: 8px;
  height: 8px;
  background: var(--divider);
  border-radius: 1px;
}
.btns {
  display: flex;
  gap: 6px;
  margin-top: 6px;
  flex-wrap: wrap;
  justify-content: center;
}
.samples {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
</style>
