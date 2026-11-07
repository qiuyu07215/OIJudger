<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { NButton, NSelect, NCheckbox, NRadioGroup, NRadioButton, useMessage, useDialog } from "naive-ui";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import { api, type Submission } from "../../api";
import { isAdmin, onNotice, state, throttle } from "../../store";
import { clock, contestTime, fmtScore } from "../../util";
import PBadge from "../../components/PBadge.vue";
import Result from "../../components/Result.vue";

const { t } = useI18n();
const router = useRouter();
const message = useMessage();
const dialog = useDialog();
const subs = ref<Submission[]>([]);
const load = throttle(async () => (subs.value = (await api.submissions().catch(() => [])).reverse()), 600);
const off = onNotice((n) => {
  if (n.type === "submission") load();
});
onMounted(load);
onBeforeUnmount(off);

const view = ref<"newest" | "unjudged" | "all">("newest");
const problem = ref<string | null>(null);
const team = ref<string | null>(null);
const includeCorrect = ref(false);
const problemOptions = computed(() => (state.info?.problems ?? []).map((p) => ({ label: `${p.letter} - ${p.title}`, value: p.name })));
const teamOptions = computed(() =>
  [...new Map(subs.value.map((s) => [s.username, s.display_name || s.username])).entries()].map(([value, label]) => ({ label, value })),
);
const rows = computed(() => {
  let r = subs.value.filter((s) => (!problem.value || s.problem === problem.value) && (!team.value || s.username === team.value));
  if (view.value === "unjudged") r = r.filter((s) => s.state !== "done");
  if (view.value === "newest") r = r.slice(0, 50);
  return r;
});
const ioi = computed(() => state.info?.format !== "icpc");

function rejudgeAll() {
  dialog.warning({
    title: t("js.rejudgeAll"),
    content: `${problem.value ?? t("common.all")} / ${team.value ?? t("common.all")}`,
    positiveText: t("common.confirm"),
    negativeText: t("common.cancel"),
    onPositiveClick: async () => {
      try {
        const r = await api.rejudgeAll(problem.value ?? "", team.value ?? "", includeCorrect.value);
        message.success(t("js.rejudged", { n: r.count }));
      } catch (e) {
        message.error(String(e));
      }
    },
  });
}
</script>

<template>
  <div class="dj-container">
    <h1 class="title">{{ t("js.title") }}</h1>
    <div class="tools">
      <n-radio-group v-model:value="view" size="small">
        <n-radio-button value="newest">{{ t("js.newest") }}</n-radio-button>
        <n-radio-button value="unjudged">{{ t("js.unjudged") }}</n-radio-button>
        <n-radio-button value="all">{{ t("js.all") }}</n-radio-button>
      </n-radio-group>
      <n-select v-model:value="problem" size="small" clearable :options="problemOptions" :placeholder="t('js.problem')" style="width: 200px" />
      <n-select v-model:value="team" size="small" clearable filterable :options="teamOptions" :placeholder="t('js.team')" style="width: 200px" />
      <template v-if="problem || team">
        <n-checkbox v-if="isAdmin()" v-model:checked="includeCorrect" size="small">{{ t("js.includeCorrect") }}</n-checkbox>
        <n-button size="small" @click="rejudgeAll">{{ t("js.rejudgeAll") }}</n-button>
      </template>
    </div>
    <table class="dj">
      <thead>
        <tr>
          <th>ID</th>
          <th>{{ t("team.time") }}</th>
          <th>{{ t("js.team") }}</th>
          <th>{{ t("team.problem") }}</th>
          <th>{{ t("team.lang") }}</th>
          <th>{{ t("js.result") }}</th>
          <th v-if="ioi" style="text-align: right">{{ t("team.score") }}</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="s in rows" :key="s.id" class="link" :class="{ ignored: s.ignored }" @click="router.push(`/jury/submissions/${s.id}`)">
          <td class="tabular">s{{ s.id }}</td>
          <td class="tabular" :title="clock(s.at_ms)">{{ contestTime(s.contest_ms) }}</td>
          <td>{{ s.display_name || s.username }}</td>
          <td><PBadge :problem="s.problem" /></td>
          <td class="langid">{{ s.lang === "c" ? "C" : "CPP" }}</td>
          <td><Result :s="s" /></td>
          <td v-if="ioi" class="tabular" style="text-align: right">{{ s.state === "done" && s.score !== null ? fmtScore(s.score) : "" }}</td>
        </tr>
        <tr v-if="!rows.length"><td :colspan="ioi ? 7 : 6" class="muted" style="text-align: center">{{ t("team.noSubmissions") }}</td></tr>
      </tbody>
    </table>
  </div>
</template>

<style scoped>
.title {
  font-weight: 500;
  font-size: 24px;
  margin: 4px 0 12px;
}
.tools {
  display: flex;
  gap: 10px;
  align-items: center;
  flex-wrap: wrap;
  margin-bottom: 12px;
}
</style>
