<script setup lang="ts">
// Problem label badge in the problem's balloon colour (DOMjudge style).
import { computed } from "vue";
import { state } from "../store";
import { contrastText } from "../util";

const props = defineProps<{ problem: string }>();
const p = computed(() => state.info?.problems.find((x) => x.name === props.problem));
</script>

<template>
  <span class="pbadge" :title="p?.title" :style="p?.color ? { background: p.color, color: contrastText(p.color) } : {}">{{ p?.letter ?? problem }}</span>
</template>
