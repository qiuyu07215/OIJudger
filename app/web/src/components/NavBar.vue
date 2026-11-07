<script setup lang="ts">
// DOMjudge-style navigation bar: brand, section links, then (right)
// notifications, Submit, Logout and the contest clock; a contest progress
// bar runs underneath.
import { computed, type Component } from "vue";
import { NBadge, NIcon } from "naive-ui";
import {
  Home, Book2, Printer, ListNumbers, CloudUpload, Logout, Login, Bell, Clock, FileCode, MessageCircle,
  Confetti as Balloon, Users, Settings, Language,
} from "@vicons/tabler";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import { api } from "../api";
import { setLocale } from "../i18n";
import { connect, enableNotifications, notifyState, now, openSubmit, refreshInfo, state } from "../store";
import { unread } from "../unread";
import { hms } from "../util";

const { t, locale } = useI18n();
const router = useRouter();

interface Link {
  to: string;
  label: string;
  icon: Component;
  badge?: number;
  exact?: boolean;
}
const links = computed<Link[]>(() => {
  const r = state.me?.role;
  if (r === "contestant")
    return [
      { to: "/team", label: t("nav.home"), icon: Home, exact: true },
      { to: "/team/problems", label: t("nav.problemset"), icon: Book2 },
      { to: "/team/print", label: t("nav.print"), icon: Printer },
      { to: "/scoreboard", label: t("nav.scoreboard"), icon: ListNumbers },
    ];
  if (r === "jury" || r === "admin")
    return [
      { to: "/jury", label: t("nav.home"), icon: Home, exact: true },
      { to: "/jury/submissions", label: t("nav.submissions"), icon: FileCode },
      { to: "/jury/clarifications", label: t("nav.clarifications"), icon: MessageCircle, badge: unread.clar },
      { to: "/balloons", label: t("nav.balloons"), icon: Balloon, badge: unread.balloons },
      { to: "/print-queue", label: t("nav.printQueue"), icon: Printer, badge: unread.prints },
      { to: "/scoreboard", label: t("nav.scoreboard"), icon: ListNumbers },
      ...(r === "admin"
        ? [
            { to: "/jury/accounts", label: t("nav.teams"), icon: Users },
            { to: "/jury/contest", label: t("nav.contest"), icon: Settings },
          ]
        : []),
    ];
  if (r === "balloon")
    return [
      { to: "/balloons", label: t("nav.balloons"), icon: Balloon, badge: unread.balloons, exact: true },
      { to: "/print-queue", label: t("nav.printQueue"), icon: Printer, badge: unread.prints },
      { to: "/scoreboard", label: t("nav.scoreboard"), icon: ListNumbers },
    ];
  return [{ to: "/scoreboard", label: t("nav.scoreboard"), icon: ListNumbers }];
});

const clock = computed(() => {
  const i = state.info;
  if (!i || i.start_ms === null || i.end_ms === null) return t("clock.notScheduled");
  if (now.value < i.start_ms) return t("clock.before", { t: hms(i.start_ms - now.value) });
  if (now.value < i.end_ms) return t("clock.left", { t: hms(i.end_ms - now.value) });
  return t("clock.over");
});
const progress = computed(() => {
  const i = state.info;
  if (!i || i.start_ms === null || i.end_ms === null) return 0;
  return Math.min(1, Math.max(0, (now.value - i.start_ms) / (i.end_ms - i.start_ms)));
});
const freezeAt = computed(() => {
  const i = state.info;
  if (!i?.freeze_at || i.start_ms === null || i.end_ms === null) return null;
  return (i.freeze_at - i.start_ms) / (i.end_ms - i.start_ms);
});
const canSubmit = computed(() => state.me?.role === "contestant" && (state.info?.problems.length ?? 0) > 0);

async function logout() {
  await api.logout().catch(() => {});
  await refreshInfo();
  connect();
  router.push("/login");
}
</script>

<template>
  <div class="nav-wrap no-print">
    <nav class="nav">
      <router-link to="/" class="brand">OIJudger</router-link>
      <div class="links">
        <router-link
          v-for="l in links"
          :key="l.to"
          :to="l.to"
          class="link"
          :exact-active-class="l.exact ? 'on' : ''"
          :active-class="l.exact ? '' : 'on'"
        >
          <n-icon :component="l.icon" size="15" />
          <n-badge :value="l.badge || 0" :show="!!l.badge" :offset="[6, -4]">{{ l.label }}</n-badge>
        </router-link>
      </div>
      <div class="right">
        <button v-if="state.me && notifyState.permission === 'default'" class="btn warn" @click="enableNotifications">
          <n-icon :component="Bell" size="14" /> {{ t("nav.notifications") }}
        </button>
        <button v-if="canSubmit" class="btn submit" @click="openSubmit()">
          <n-icon :component="CloudUpload" size="14" /> {{ t("nav.submit") }}
        </button>
        <button v-if="state.me" class="btn info" :title="state.me.display_name || state.me.username" @click="logout">
          <n-icon :component="Logout" size="14" /> {{ t("nav.logout") }}
        </button>
        <router-link v-else to="/login" class="btn info"><n-icon :component="Login" size="14" /> {{ t("nav.login") }}</router-link>
        <button class="lang" :title="locale === 'en' ? '中文' : 'English'" @click="setLocale(locale === 'en' ? 'zh-CN' : 'en')">
          <n-icon :component="Language" size="15" />
        </button>
        <span class="clock tabular"><n-icon :component="Clock" size="14" /> {{ clock }}</span>
      </div>
    </nav>
    <div class="progress" :title="`${Math.round(progress * 100)}%`">
      <span class="done" :style="{ width: progress * 100 + '%' }" />
      <span v-if="freezeAt !== null" class="freeze" :style="{ left: freezeAt * 100 + '%' }" />
    </div>
  </div>
</template>

<style scoped>
.nav-wrap {
  position: sticky;
  top: 0;
  z-index: 30;
}
.nav {
  background: var(--nav-bg);
  color: var(--nav-fg);
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 0 14px;
  min-height: 46px;
  flex-wrap: wrap;
}
.brand {
  font-size: 19px;
  font-weight: 500;
  text-decoration: none;
  color: #faf9f5;
  font-family: var(--serif);
}
.links {
  display: flex;
  align-items: center;
  gap: 2px;
  flex-wrap: wrap;
}
.link {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 6px 9px;
  text-decoration: none;
  color: var(--nav-muted);
  font-size: 14px;
  border-radius: 4px;
}
.link :deep(.n-badge) {
  color: inherit;
}
.link:hover,
.link.on {
  color: #faf9f5;
}
.link.on {
  background: rgba(250, 249, 245, 0.08);
}
.right {
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  padding: 6px 0;
}
.btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font: inherit;
  font-size: 13px;
  padding: 4px 10px;
  border-radius: 4px;
  border: 0;
  cursor: pointer;
  color: #fff;
  text-decoration: none;
}
.btn.submit {
  background: var(--btn-submit);
}
.btn.info {
  background: var(--btn-info);
}
.btn.warn {
  background: var(--btn-warn);
}
.btn:hover {
  filter: brightness(1.1);
}
.lang {
  background: none;
  border: 0;
  color: var(--nav-muted);
  cursor: pointer;
  display: inline-flex;
  padding: 4px;
}
.clock {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 13.5px;
  color: var(--nav-fg);
  white-space: nowrap;
}
.progress {
  position: relative;
  height: 4px;
  background: var(--divider);
}
.progress .done {
  position: absolute;
  left: 0;
  top: 0;
  bottom: 0;
  background: var(--accent);
}
.progress .freeze {
  position: absolute;
  top: 0;
  bottom: 0;
  width: 2px;
  background: var(--sb-frozen);
}
</style>
