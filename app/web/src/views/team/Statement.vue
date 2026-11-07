<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { NButton, NEmpty, NSpin } from "naive-ui";
import { useI18n } from "vue-i18n";
import { useRoute } from "vue-router";
import { renderStatement } from "../../markdown";
import { openSubmit, state } from "../../store";
import { contrastText, fmtScore } from "../../util";

const { t } = useI18n();
const route = useRoute();
const name = computed(() => String(route.params.name));
const p = computed(() => state.info?.problems.find((x) => x.name === name.value));
const statementUrl = computed(() => `/api/problems/${encodeURIComponent(name.value)}/statement`);

const html = ref("");
const loading = ref(false);
watch(
  [name, () => p.value?.statement],
  async () => {
    html.value = "";
    if (p.value?.statement !== "md") return;
    loading.value = true;
    try {
      const text = await (await fetch(statementUrl.value)).text();
      html.value = renderStatement(text, name.value);
    } finally {
      loading.value = false;
    }
  },
  { immediate: true },
);

</script>

<template>
  <div class="dj-container statement-page">
    <n-empty v-if="!p" :description="t('problemset.notStarted')" style="margin-top: 60px" />
    <template v-else>
      <div class="head">
        <span class="letter" :style="p.color ? { background: p.color, color: contrastText(p.color) } : {}">{{ p.letter }}</span>
        <div>
          <h1 class="page-title" style="margin: 0">{{ p.title }}</h1>
          <div class="muted tabular meta">
            {{ t("problemset.limits", { t: p.time_ms / 1000, m: p.memory_mb }) }}
            <template v-if="!p.stdio">{{ p.input_file }} / {{ p.output_file }}</template>
            <template v-if="state.info?.format !== 'icpc'"> · {{ t("problemset.full", { n: fmtScore(p.full) }) }}</template>
          </div>
        </div>
        <span style="flex: 1" />
        <n-button v-if="state.me?.role === 'contestant'" type="success" @click="openSubmit(p.name)">{{ t("problemset.submit") }}</n-button>
      </div>

      <div class="layout">
        <section class="card statement">
          <template v-if="p.statement === 'pdf'">
            <div class="pdf-bar">
              <a :href="statementUrl" target="_blank"><n-button size="small">{{ t("problemset.statement") }}</n-button></a>
            </div>
            <iframe :src="statementUrl" class="pdf" title="statement" />
          </template>
          <iframe v-else-if="p.statement === 'html'" :src="statementUrl" class="html" sandbox="" title="statement" />
          <n-spin v-else-if="p.statement === 'md'" :show="loading">
            <article class="md" v-html="html" />
          </n-spin>
          <div v-else class="muted">{{ t("problemset.noStatement") }}</div>

          <div v-if="p.samples.length" class="samples">
            <div class="label">{{ t("problemset.samples") }}</div>
            <a
              v-for="f in p.samples"
              :key="f"
              :href="`/api/problems/${encodeURIComponent(p.name)}/samples/${encodeURIComponent(f)}`"
              download
              class="sample mono"
            >{{ f }}</a>
          </div>
        </section>

      </div>
    </template>
  </div>
</template>

<style scoped>
.head {
  display: flex;
  align-items: center;
  gap: 14px;
  margin-bottom: 16px;
}
.letter {
  width: 46px;
  height: 46px;
  border-radius: 12px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-weight: 700;
  font-size: 22px;
  background: var(--accent-soft);
  flex: none;
}
.meta {
  font-size: 13px;
  margin-top: 4px;
}
.layout {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  max-width: 1000px;
  margin: 0 auto;
  gap: 16px;
  align-items: start;
}
aside {
  display: flex;
  flex-direction: column;
  gap: 12px;
  position: sticky;
  top: 68px;
}
@media (max-width: 980px) {
  .layout {
    grid-template-columns: 1fr;
  }
  aside {
    position: static;
  }
}
.label {
  font-weight: 600;
  color: var(--text2);
  margin-bottom: 10px;
}
.pdf-bar {
  display: flex;
  justify-content: flex-end;
  margin-bottom: 8px;
}
.pdf,
.html {
  width: 100%;
  height: 78vh;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: #fff;
}
.md {
  line-height: 1.75;
  font-size: 15px;
  overflow-wrap: anywhere;
}
.md :deep(pre) {
  background: var(--raised);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 10px 12px;
  overflow: auto;
  font-family: var(--mono);
  font-size: 13px;
}
.md :deep(code) {
  font-family: var(--mono);
}
.md :deep(img) {
  max-width: 100%;
}
.md :deep(table) {
  border-collapse: collapse;
}
.md :deep(th),
.md :deep(td) {
  border: 1px solid var(--border);
  padding: 4px 10px;
}
.md :deep(.math-block) {
  overflow-x: auto;
}
.samples {
  margin-top: 18px;
  border-top: 1px solid var(--divider);
  padding-top: 14px;
}
.sample {
  display: inline-block;
  margin: 0 8px 8px 0;
  padding: 4px 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
  text-decoration: none;
  font-size: 13px;
}
.sample:hover {
  border-color: var(--accent);
}
.sub-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 6px 4px;
  border-top: 1px solid var(--divider);
  cursor: pointer;
  font-size: 13.5px;
}
.sub-row:first-of-type {
  border-top: 0;
}
.sub-row:hover {
  background: var(--accent-soft);
}
.grow {
  flex: 1;
}
</style>
