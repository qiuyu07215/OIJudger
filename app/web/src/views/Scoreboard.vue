<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { NAlert, NButton, NEmpty, NSpin, NPopover, NCheckbox, NIcon } from "naive-ui";
import { Filter } from "@vicons/tabler";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";
import { api, type Scoreboard } from "../api";
import { onNotice, state, throttle } from "../store";
import { clock } from "../util";
import BoardTable from "../components/BoardTable.vue";

const { t } = useI18n();
const route = useRoute();
const router = useRouter();
const board = ref<Scoreboard | null>(null);
const error = ref("");
const projector = computed(() => route.query.projector !== undefined);

// DOMjudge reloads the scoreboard periodically; here pushes trigger reloads
// and a 30 s timer covers missed ones.
const load = throttle(async () => {
  try {
    board.value = await api.scoreboard();
    error.value = "";
  } catch (e) {
    error.value = String(e);
  }
}, 1500);
const off = onNotice((n) => {
  if (n.type === "scoreboard" || n.type === "contest") load();
});
let timer: ReturnType<typeof setInterval> | undefined;
let scroller: ReturnType<typeof setInterval> | undefined;
onMounted(() => {
  load();
  timer = setInterval(load, 30000);
  if (projector.value) {
    let pause = 0;
    scroller = setInterval(() => {
      if (pause > 0) return void pause--;
      if (window.innerHeight + window.scrollY >= document.body.scrollHeight - 2) {
        pause = 40;
        setTimeout(() => window.scrollTo({ top: 0, behavior: "smooth" }), 4000);
      } else window.scrollBy(0, 1);
    }, 50);
  }
});
onBeforeUnmount(() => {
  off();
  clearInterval(timer);
  clearInterval(scroller);
});

// Filter by affiliation.
const groups = computed(() => [...new Set((board.value?.rows ?? []).map((r) => r.group_name).filter(Boolean))].sort());
const picked = ref<string[]>([]);
const shown = computed(() => {
  const b = board.value;
  if (!b || !picked.value.length) return b;
  return { ...b, rows: b.rows.filter((r) => picked.value.includes(r.group_name)) };
});
const info = computed(() => state.info);
</script>

<template>
  <div class="dj-container">
    <div class="dj-card head">
      <div class="dj-card-header">
        <b class="cname">{{ info?.title }}</b>
        <span class="grow" />
        <span v-if="info?.start_ms && info.end_ms" class="muted tabular small">{{ t("board.started", { s: clock(info.start_ms).slice(0, 5), e: clock(info.end_ms).slice(0, 5) }) }}</span>
      </div>
    </div>
    <n-alert v-if="board?.frozen" type="warning" :show-icon="false" class="banner">{{ t("board.frozen", { m: info?.freeze_min ?? 0 }) }}</n-alert>
    <n-alert v-if="board?.hidden" type="info" :show-icon="false" class="banner">{{ t("board.hidden") }}</n-alert>

    <div v-if="!projector" class="tools">
      <n-popover v-if="groups.length" trigger="click" placement="bottom-start">
        <template #trigger>
          <n-button size="small" secondary>
            <template #icon><n-icon :component="Filter" /></template>{{ t("board.filter") }}{{ picked.length ? ` (${picked.length})` : "" }}
          </n-button>
        </template>
        <div class="filter">
          <b>{{ t("board.affiliation") }}</b>
          <n-checkbox v-for="g in groups" :key="g" :checked="picked.includes(g)" @update:checked="(v: boolean) => (picked = v ? [...picked, g] : picked.filter((x) => x !== g))">{{ g }}</n-checkbox>
          <n-button size="tiny" quaternary @click="picked = []">{{ t("common.reset") }}</n-button>
        </div>
      </n-popover>
      <span class="grow" />
      <n-button size="small" quaternary @click="router.push({ query: { projector: '1' } })">{{ t("nav.projector") }}</n-button>
    </div>

    <n-empty v-if="error" :description="error" style="margin-top: 40px" />
    <n-spin v-else :show="!board">
      <BoardTable v-if="shown" :board="shown" :highlight="state.me?.id" :large="projector" />
      <n-empty v-if="board && !board.rows.length" :description="t('board.empty')" style="margin-top: 30px" />
    </n-spin>
  </div>
</template>

<style scoped>
.head {
  margin-bottom: 12px;
}
.head .dj-card-header {
  border-bottom: 0;
  border-radius: 6px;
}
.cname {
  font-size: 15px;
}
.grow {
  flex: 1;
}
.small {
  font-size: 12.5px;
}
.banner {
  margin-bottom: 12px;
}
.tools {
  display: flex;
  margin-bottom: 10px;
}
.filter {
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 360px;
  overflow: auto;
}
</style>
