<script setup lang="ts">
// ICPC resolver ("滚榜"): reveal frozen cells from the lowest-ranked team
// upward. Each step first highlights the cell, then reveals it and
// re-sorts the board. Space / → advances.
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { NButton, NEmpty, NSpin } from "naive-ui";
import { useI18n } from "vue-i18n";
import { api, type ResolveStep, type Scoreboard } from "../../api";
import { state } from "../../store";
import BoardTable from "../../components/BoardTable.vue";

const { t } = useI18n();
const board = ref<Scoreboard | null>(null);
const steps = ref<ResolveStep[]>([]);
const index = ref(0);
const focus = ref<{ account: number; problem: number } | null>(null);
const error = ref("");
let original: Scoreboard | null = null;

async function load() {
  try {
    const d = await api.resolver();
    original = d.board;
    steps.value = d.steps;
    restart();
  } catch (e) {
    error.value = String(e);
  }
}

function restart() {
  if (!original) return;
  board.value = JSON.parse(JSON.stringify(original));
  index.value = 0;
  focus.value = null;
}

function rerank(b: Scoreboard) {
  b.rows.sort((x, y) => y.solved - x.solved || x.penalty - y.penalty || x.last_ac_ms - y.last_ac_ms);
  let rank = 0;
  b.rows.forEach((r, i) => {
    const p = b.rows[i - 1];
    if (!p || p.solved !== r.solved || p.penalty !== r.penalty || p.last_ac_ms !== r.last_ac_ms) rank = i + 1;
    r.rank = rank;
  });
}

async function next() {
  const b = board.value;
  if (!b || index.value >= steps.value.length) return;
  const s = steps.value[index.value];
  if (!focus.value) {
    focus.value = { account: s.account, problem: s.problem };
    await nextTick();
    document.querySelector(".cell.focus")?.scrollIntoView({ behavior: "smooth", block: "center" });
    return;
  }
  const row = b.rows.find((r) => r.account === s.account);
  if (row) {
    row.cells[s.problem] = s.cell;
    row.solved = s.solved;
    row.penalty = s.penalty;
    row.last_ac_ms = s.last_ac_ms;
  }
  rerank(b);
  index.value++;
  focus.value = null;
}

const remaining = computed(() => steps.value.length - index.value);
const auto = ref<ReturnType<typeof setInterval> | null>(null);
function toggleAuto() {
  if (auto.value) {
    clearInterval(auto.value);
    auto.value = null;
  } else {
    auto.value = setInterval(() => {
      if (remaining.value === 0) toggleAuto();
      else void next();
    }, 900);
  }
}

function onKey(e: KeyboardEvent) {
  if (e.key === " " || e.key === "ArrowRight" || e.key === "Enter") {
    e.preventDefault();
    void next();
  }
}
onMounted(() => {
  load();
  window.addEventListener("keydown", onKey);
});
onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKey);
  if (auto.value) clearInterval(auto.value);
});
</script>

<template>
  <div class="page wide">
    <div class="head">
      <h1 class="page-title" style="margin: 0">{{ state.info?.title }} · {{ t("resolver.title") }}</h1>
      <span class="grow" />
      <span class="muted">{{ remaining ? t("resolver.remaining", { n: remaining }) : t("resolver.done") }}</span>
      <n-button :disabled="!remaining" @click="next">{{ t("resolver.next") }}</n-button>
      <n-button :disabled="!remaining" @click="toggleAuto">{{ auto ? t("resolver.pause") : t("resolver.auto") }}</n-button>
      <n-button quaternary @click="restart">{{ t("resolver.restart") }}</n-button>
    </div>
    <n-empty v-if="state.info?.format !== 'icpc'" :description="t('resolver.notIcpc')" />
    <n-empty v-else-if="error" :description="error" />
    <n-spin v-else :show="!board">
      <BoardTable v-if="board" :board="board" :focus="focus" :highlight="focus?.account ?? null" large />
    </n-spin>
  </div>
</template>

<style scoped>
.head {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 14px;
  flex-wrap: wrap;
}
.grow {
  flex: 1;
}
</style>
