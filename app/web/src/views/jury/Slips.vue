<script setup lang="ts">
// Printable password slips (cut along the dashed lines).
import { onMounted, ref } from "vue";
import { NButton } from "naive-ui";
import { useI18n } from "vue-i18n";
import type { Credential } from "../../api";
import { state } from "../../store";

const { t } = useI18n();
const creds = ref<Credential[]>([]);
const url = window.location.origin;
const doPrint = () => window.print();
onMounted(() => {
  try {
    creds.value = JSON.parse(sessionStorage.getItem("oij.slips") || "[]");
  } catch {
    creds.value = [];
  }
});
</script>

<template>
  <div class="wrap">
    <div class="no-print bar">
      <n-button type="primary" @click="doPrint">{{ t("print.printNow") }}</n-button>
    </div>
    <div class="grid">
      <div v-for="c in creds" :key="c.username" class="slip">
        <div class="title">{{ t("acc.slipTitle", { title: state.info?.title || "OIJudger" }) }}</div>
        <div class="who">{{ c.display_name || c.username }}<span v-if="c.seat"> · {{ c.seat }}</span></div>
        <table>
          <tr><td>{{ t("acc.url") }}</td><td class="mono">{{ url }}</td></tr>
          <tr><td>{{ t("login.username") }}</td><td class="mono"><b>{{ c.username }}</b></td></tr>
          <tr><td>{{ t("login.password") }}</td><td class="mono"><b>{{ c.password }}</b></td></tr>
        </table>
      </div>
    </div>
  </div>
</template>

<style scoped>
.wrap {
  padding: 16px;
  background: #fff;
  color: #000;
  min-height: 100vh;
}
.bar {
  margin-bottom: 12px;
}
.grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
}
.slip {
  border: 1px dashed #999;
  padding: 12px 14px;
  break-inside: avoid;
}
.title {
  font-size: 12px;
  color: #555;
}
.who {
  font-weight: 600;
  margin: 4px 0 6px;
}
td {
  padding: 1px 12px 1px 0;
  font-size: 14px;
}
.mono {
  font-family: var(--mono);
  font-size: 15px;
}
</style>
