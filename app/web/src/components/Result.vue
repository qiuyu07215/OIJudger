<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import type { Submission } from "../api";
import { djResult } from "../util";

const props = defineProps<{ s: Pick<Submission, "state" | "outcome" | "label"> }>();
const { t } = useI18n();
const r = computed(() => djResult(props.s));
</script>

<template>
  <span class="sol" :class="r.cls" :title="s.label">{{ t(`result.${r.key}`) }}</span>
</template>
