import { createRouter, createWebHistory, type RouteRecordRaw } from "vue-router";
import { refreshInfo, state } from "./store";

declare module "vue-router" {
  interface RouteMeta {
    /** Needs a login. */
    auth?: boolean;
    /** Needs a role: "team" (contestant), "staff" (not contestant), "jury" (jury/admin), "admin". */
    role?: "team" | "staff" | "jury" | "admin";
    /** No navigation bar (resolver, print pages). */
    bare?: boolean;
  }
}

// Same page structure as DOMjudge: /team (team interface), /jury (jury
// interface), public scoreboard, balloons and printing for volunteers.
const routes: RouteRecordRaw[] = [
  { path: "/", name: "home", component: { render: () => null } },
  { path: "/login", name: "login", component: () => import("./views/Login.vue") },
  { path: "/team", name: "team", component: () => import("./views/team/Home.vue"), meta: { auth: true, role: "team" } },
  { path: "/team/problems", name: "problemset", component: () => import("./views/team/Problemset.vue"), meta: { auth: true } },
  { path: "/team/problems/:name", name: "statement", component: () => import("./views/team/Statement.vue"), meta: { auth: true } },
  { path: "/team/print", name: "print", component: () => import("./views/team/Print.vue"), meta: { auth: true, role: "team" } },
  { path: "/scoreboard", name: "scoreboard", component: () => import("./views/Scoreboard.vue") },
  { path: "/balloons", name: "balloons", component: () => import("./views/Balloons.vue"), meta: { auth: true, role: "staff" } },
  { path: "/print-queue", name: "print-queue", component: () => import("./views/PrintQueue.vue"), meta: { auth: true, role: "staff" } },
  { path: "/print-job/:id", name: "print-job", component: () => import("./views/PrintJob.vue"), meta: { auth: true, role: "staff", bare: true } },
  { path: "/jury", name: "jury", component: () => import("./views/jury/Home.vue"), meta: { auth: true, role: "jury" } },
  { path: "/jury/submissions", name: "jury-submissions", component: () => import("./views/jury/Submissions.vue"), meta: { auth: true, role: "jury" } },
  { path: "/jury/submissions/:id", name: "jury-submission", component: () => import("./views/jury/Submission.vue"), meta: { auth: true, role: "jury" } },
  { path: "/jury/clarifications", name: "jury-clarifications", component: () => import("./views/jury/Clarifications.vue"), meta: { auth: true, role: "jury" } },
  { path: "/jury/accounts", name: "jury-accounts", component: () => import("./views/jury/Accounts.vue"), meta: { auth: true, role: "admin" } },
  { path: "/jury/contest", name: "jury-contest", component: () => import("./views/jury/Settings.vue"), meta: { auth: true, role: "admin" } },
  { path: "/jury/resolver", name: "resolver", component: () => import("./views/jury/Resolver.vue"), meta: { auth: true, role: "jury", bare: true } },
  { path: "/jury/slips", name: "slips", component: () => import("./views/jury/Slips.vue"), meta: { auth: true, role: "jury", bare: true } },
  { path: "/:rest(.*)*", redirect: "/" },
];

export const router = createRouter({ history: createWebHistory(), routes });

/** Where a user lands after logging in. */
export function homeFor(): string {
  const r = state.me?.role;
  if (!r) return state.info?.public_scoreboard ? "/scoreboard" : "/login";
  if (r === "jury" || r === "admin") return "/jury";
  if (r === "balloon") return "/balloons";
  return "/team";
}

router.beforeEach(async (to) => {
  if (!state.loaded) {
    try {
      await refreshInfo();
    } catch {
      /* server unreachable: pages show their own errors */
    }
  }
  if (to.name === "home") return homeFor();
  if (to.meta.auth && !state.me) return { name: "login", query: { next: to.fullPath } };
  const role = state.me?.role;
  const jury = role === "jury" || role === "admin";
  if (to.meta.role === "team" && role !== "contestant") return homeFor();
  if (to.meta.role === "staff" && role === "contestant") return homeFor();
  if (to.meta.role === "jury" && !jury) return homeFor();
  if (to.meta.role === "admin" && role !== "admin") return homeFor();
  return true;
});
