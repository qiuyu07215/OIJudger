import { reactive } from "vue";

/** Badges in the top bar; pages reset their counter when shown. */
export const unread = reactive({ clar: 0, balloons: 0, prints: 0 });
