<script setup lang="ts">
// Jury view of one submission (DOMjudge style): info table, actions
// (rejudge, ignore), testcase result boxes with details, compiler output,
// source.
import { computed, onBeforeUnmount, ref, watch } from "vue";
import { NButton, NSpin, NEmpty, useMessage } from "naive-ui";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";
import { api, type SubmissionDetail } from "../../api";
import { onNotice, state } from "../../store";
import { clock, contestTime, fmtScore } from "../../util";
import PBadge from "../../components/PBadge.vue";
import Result from "../../components/Result.vue";

const { t } = useI18n();
const route = useRoute();
const router = useRouter();
const message = useMessage();
const id = computed(() => Number(route.params.id));
const d = ref<SubmissionDetail | null>(null);
const error = ref("");
const open = ref<string | null>(null);

async function load() {
  try {
    d.value = await api.submission(id.value);
    error.value = "";
  } catch (e) {
    error.value = String(e);
  }
}
watch(id, load, { immediate: true });
const off = onNotice((n) => {
  if (n.type === "submission" && n.id === id.value) load();
});
onBeforeUnmount(off);

const p = computed(() => state.info?.problems.find((x) => x.name === d.value?.problem));
const caseCls = (v: string) => (v === "AC" ? "correct" : v === "Skipped" ? "skipped" : v === "PC" ? "partial" : "incorrect");
async function rejudge() {
  await api.rejudge([id.value]);
  message.success(t("js.rejudged", { n: 1 }));
}
async function toggleIgnore() {
  if (!d.value) return;
  await api.ignore(id.value, !d.value.ignored);
  load();
}
async function copy() {
  await navigator.clipboard.writeText(d.value?.source ?? "");
  message.success(t("common.copied"));
}
</script>

<template>
  <div class="dj-container">
    <n-empty v-if="error" :description="error" style="margin-top: 60px" />
    <n-spin v-else :show="!d">
      <template v-if="d">
        <div class="head">
          <n-button size="small" quaternary @click="router.back()">← {{ t("common.back") }}</n-button>
          <h1 class="title" :class="{ struck: d.ignored }">{{ t("sub.title", { id: d.id }) }}</h1>
          <span v-if="d.ignored" class="sol queued">{{ t("js.ignored") }}</span>
          <span class="grow" />
          <n-button size="small" @click="rejudge">{{ t("js.rejudge") }}</n-button>
          <n-button size="small" @click="toggleIgnore">{{ d.ignored ? t("js.unignore") : t("js.ignore") }}</n-button>
        </div>

        <table class="dj info">
          <tbody>
            <tr><th>{{ t("js.team") }}</th><td>{{ d.display_name || d.username }} <span class="muted">({{ d.username }})</span></td></tr>
            <tr><th>{{ t("sub.problem") }}</th><td><PBadge :problem="d.problem" /> {{ p?.title }}</td></tr>
            <tr><th>{{ t("sub.lang") }}</th><td class="langid">{{ d.lang === "c" ? "C" : "CPP" }}</td></tr>
            <tr><th>{{ t("sub.time") }}</th><td class="tabular">{{ clock(d.at_ms) }} ({{ contestTime(d.contest_ms) }})</td></tr>
            <tr>
              <th>{{ t("sub.result") }}</th>
              <td>
                <Result :s="d" />
                <span v-if="d.state === 'done' && d.score !== null" class="tabular"> · {{ fmtScore(d.score) }} / {{ fmtScore(d.full) }}</span>
                <span v-if="d.state === 'done' && d.time_ms" class="muted tabular"> · {{ d.time_ms }} ms</span>
              </td>
            </tr>
          </tbody>
        </table>

        <template v-if="d.subtasks.length">
          <h3>{{ t("js.testcases") }}</h3>
          <div v-for="(st, si) in d.subtasks" :key="si" class="st">
            <span class="st-label">{{ t("sub.subtask", { n: si + 1 }) }} <span class="muted tabular">{{ fmtScore(st.score) }}/{{ fmtScore(st.full) }}</span></span>
            <span
              v-for="(c, ci) in st.cases"
              :key="ci"
              class="case"
              :class="[caseCls(c.verdict), { on: open === `${si}-${ci}` }]"
              :title="`${c.verdict} · ${c.time_ms} ms`"
              @click="open = open === `${si}-${ci}` ? null : `${si}-${ci}`"
            >{{ ci + 1 }}</span>
            <div v-for="(c, ci) in st.cases" v-show="open === `${si}-${ci}`" :key="`d${ci}`" class="dj-card case-detail">
              <div><b>{{ c.verdict }}</b> · {{ c.time_ms }} ms · {{ (c.memory_kb / 1024).toFixed(1) }} MiB</div>
              <div v-if="c.input" class="muted mono small">{{ t("js.input") }}: {{ c.input }}</div>
              <pre v-if="c.message" class="code small">{{ c.message }}</pre>
            </div>
          </div>
        </template>

        <template v-if="d.message">
          <h3>{{ t("sub.compile") }}</h3>
          <pre class="code">{{ d.message }}</pre>
        </template>

        <h3>{{ t("sub.source") }} <n-button size="tiny" quaternary @click="copy">{{ t("common.copy") }}</n-button></h3>
        <pre class="code">{{ d.source }}</pre>
      </template>
    </n-spin>
  </div>
</template>

<style scoped>
.head {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 12px;
  flex-wrap: wrap;
}
.title {
  font-weight: 500;
  font-size: 22px;
  margin: 0;
}
.title.struck {
  text-decoration: line-through;
}
.grow {
  flex: 1;
}
table.info {
  max-width: 640px;
}
table.info th {
  width: 120px;
  border-bottom: 1px solid var(--divider);
}
h3 {
  font-size: 15px;
  margin: 20px 0 8px;
  display: flex;
  align-items: center;
  gap: 8px;
}
.st {
  margin-bottom: 8px;
}
.st-label {
  display: inline-block;
  min-width: 120px;
  font-size: 13px;
}
.case {
  display: inline-block;
  min-width: 24px;
  padding: 3px 4px;
  margin: 0 3px 3px 0;
  border-radius: 3px;
  text-align: center;
  font-size: 11.5px;
  color: #fff;
  cursor: pointer;
}
.case.on {
  outline: 2px solid var(--text);
}
.case.correct {
  background: var(--sb-first);
}
.case.incorrect {
  background: #c0533c;
}
.case.partial {
  background: #c98a2b;
}
.case.skipped {
  background: var(--text3);
}
.case-detail {
  padding: 8px 12px;
  margin: 4px 0 8px;
  font-size: 13px;
}
.small {
  font-size: 12px;
}
</style>
