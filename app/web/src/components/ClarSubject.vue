<script setup lang="ts">
// Clarification subject: "General issue", "Technical issue" or a problem badge.
import { useI18n } from "vue-i18n";
import PBadge from "./PBadge.vue";

defineProps<{ problem: string }>();
const { t } = useI18n();
</script>

<template>
  <span v-if="!problem">{{ t("team.general") }}</span>
  <span v-else-if="problem === '#technical'">{{ t("team.technical") }}</span>
  <span v-else class="nowrap">{{ t("team.problem") }} <PBadge :problem="problem" /></span>
</template>

<style scoped>
.nowrap {
  white-space: nowrap;
}
</style>
