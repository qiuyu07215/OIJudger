<script setup lang="ts">
// Badges and notifications: new clarifications (in-page and, once enabled,
// browser notifications as in DOMjudge), balloons and print jobs for staff.
import { onBeforeUnmount } from "vue";
import { useNotification } from "naive-ui";
import { useI18n } from "vue-i18n";
import { useRoute } from "vue-router";
import { api } from "../api";
import { browserNotify, isJury, isStaff, onNotice, state } from "../store";
import { unread } from "../unread";

const notification = useNotification();
const { t } = useI18n();
const route = useRoute();

const off = onNotice(async (n) => {
  if (!state.me) return;
  if (n.type === "clarification") {
    if (isJury()) {
      if (n.from !== null && route.name !== "jury-clarifications") {
        unread.clar++;
        browserNotify(t("nav.clarifications"), t("clar.newRequests"));
      }
      return;
    }
    const list = await api.clarifications().catch(() => []);
    const c = list.find((x) => x.id === n.id);
    if (c && c.answer) {
      if (route.name !== "team") unread.clar++;
      notification.info({ title: t("nav.clarifications"), content: c.answer, meta: c.question || undefined, duration: 0, keepAliveOnHover: true });
      browserNotify(t("nav.clarifications"), c.answer.slice(0, 120));
    }
  } else if (n.type === "announcement") {
    const list = await api.announcements().catch(() => []);
    const a = list.find((x) => x.id === n.id);
    if (a) {
      notification.info({ title: a.title, content: a.body || undefined, duration: 0, keepAliveOnHover: true });
      browserNotify(a.title, a.body);
    }
  } else if (n.type === "balloon" && isStaff() && route.name !== "balloons") {
    unread.balloons++;
  } else if (n.type === "print" && isStaff() && route.name !== "print-queue") {
    unread.prints++;
  }
});
onBeforeUnmount(off);
</script>

<template>
  <span style="display: none" />
</template>
