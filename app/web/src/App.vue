<script setup lang="ts">
import { computed } from "vue";
import {
  NConfigProvider, NMessageProvider, NDialogProvider, NNotificationProvider, darkTheme, useOsTheme,
  zhCN, dateZhCN, enUS, dateEnUS,
} from "naive-ui";
import { useI18n } from "vue-i18n";
import { useRoute } from "vue-router";
import { overrides } from "./theme";
import NavBar from "./components/NavBar.vue";
import Notifier from "./components/Notifier.vue";
import SubmitDialog from "./components/SubmitDialog.vue";

const os = useOsTheme();
const dark = computed(() => os.value === "dark");
const { locale } = useI18n();
const route = useRoute();
</script>

<template>
  <n-config-provider
    :theme="dark ? darkTheme : null"
    :theme-overrides="overrides(dark)"
    :locale="locale === 'en' ? enUS : zhCN"
    :date-locale="locale === 'en' ? dateEnUS : dateZhCN"
  >
    <n-message-provider placement="top">
      <n-notification-provider placement="top-right">
        <n-dialog-provider>
          <NavBar v-if="!route.meta.bare && route.query.projector === undefined" />
          <Notifier />
          <SubmitDialog />
          <router-view />
        </n-dialog-provider>
      </n-notification-provider>
    </n-message-provider>
  </n-config-provider>
</template>
