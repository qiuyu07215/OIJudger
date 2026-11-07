<script setup lang="ts">
// Scoreboard table as DOMjudge lays it out: rank (medal colours), team with
// affiliation, score (solved + time; points for IOI/OI), then one column
// per problem with a square label in the balloon colour. Cells: solved
// (first-to-solve darker), tried and incorrect, tried and pending
// ("a + p tries"), untried. Summary row and legend at the bottom.
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import type { BoardRow, Cell, Scoreboard } from "../api";
import { state } from "../store";
import { contrastText, fmtScore } from "../util";

const props = defineProps<{
  board: Scoreboard;
  /** Own team (highlighted row). */
  highlight?: number | null;
  /** Resolver: the cell being revealed. */
  focus?: { account: number; problem: number } | null;
  large?: boolean;
  /** Hide summary row and legend (team page row). */
  compact?: boolean;
}>();
const { t } = useI18n();
const icpc = computed(() => props.board.format === "icpc");

function cellClass(c: Cell, full: number) {
  if (icpc.value) {
    if (c.solved) return c.first ? "first" : "solved";
    if (c.pending > 0) return "pending";
    if (c.attempts > 0) return "wrong";
    return "";
  }
  if (c.score === null) return c.pending > 0 ? "pending" : "";
  if (c.score >= full && full > 0) return "solved";
  if (c.score <= 0) return "wrong";
  return "partial";
}
function partialStyle(c: Cell, full: number) {
  if (icpc.value || c.score === null || full <= 0 || c.score <= 0 || c.score >= full) return {};
  const r = c.score / full;
  return { background: `color-mix(in srgb, var(--sb-solved) ${Math.round(30 + r * 60)}%, var(--sb-wrong))` };
}
const tries = (n: number) => (n === 1 ? t("board.try1") : t("board.tries", { n }));
function cellTitle(r: BoardRow, i: number) {
  const c = r.cells[i];
  const p = props.board.problems[i];
  if (icpc.value) {
    if (c.solved) return `${p.letter}: ${c.time_min} min, ${tries(c.attempts)}${c.first ? ` (${t("board.solvedFirst")})` : ""}`;
    if (c.pending) return `${p.letter}: ${t("board.pending", { a: c.attempts - c.pending, p: c.pending })}`;
    return c.attempts ? `${p.letter}: ${tries(c.attempts)}` : "";
  }
  return c.score !== null ? `${p.letter}: ${fmtScore(c.score)} / ${fmtScore(p.full)}` : "";
}

/** Medal for a rank (gold, silver, bronze counts from the contest settings). */
function medal(rank: number) {
  const [g, s, b] = state.info?.medals ?? [0, 0, 0];
  if (!rank || props.board.frozen) return "";
  if (rank <= g) return "gold";
  if (rank <= g + s) return "silver";
  if (rank <= g + s + b) return "bronze";
  return "";
}

const summary = computed(() =>
  props.board.problems.map((p, i) => {
    let solved = 0, tried = 0, pending = 0, sum = 0;
    for (const r of props.board.rows) {
      const c = r.cells[i];
      if (c.solved) solved++;
      tried += c.attempts;
      pending += c.pending;
      sum += c.score ?? 0;
    }
    return { solved, tried, pending, avg: sum / (props.board.rows.length || 1), full: p.full };
  }),
);
</script>

<template>
  <div class="wrap" :class="{ large }">
    <table class="sb tabular">
      <thead>
        <tr>
          <th class="rank" :title="t('board.rank')">{{ t("board.rank").toUpperCase() }}</th>
          <th class="team">{{ t("board.team").toUpperCase() }}</th>
          <th :colspan="icpc ? 2 : 1" class="score">{{ t("board.score").toUpperCase() }}</th>
          <th v-for="p in board.problems" :key="p.name" class="prob" :title="`${p.letter}: ${p.title}`">
            <span class="badge" :style="p.color ? { background: p.color, color: contrastText(p.color) } : {}">{{ p.letter }}</span>
          </th>
        </tr>
      </thead>
      <TransitionGroup tag="tbody" name="row">
        <tr v-for="r in board.rows" :key="r.account" :class="{ me: highlight === r.account }">
          <td class="rank" :class="medal(r.rank)">{{ r.rank ? r.rank : "?" }}</td>
          <td class="team">
            <div class="name">{{ r.display_name }}</div>
            <div v-if="r.group_name" class="aff">{{ r.group_name }}</div>
          </td>
          <template v-if="icpc">
            <td class="solved-n">{{ r.solved }}</td>
            <td class="time">{{ r.penalty }}</td>
          </template>
          <td v-else class="solved-n">{{ board.hidden ? "" : fmtScore(r.total) }}</td>
          <td
            v-for="(c, i) in r.cells"
            :key="i"
            class="cell"
            :class="[cellClass(c, board.problems[i].full), { focus: focus && focus.account === r.account && focus.problem === i }]"
            :style="partialStyle(c, board.problems[i].full)"
            :title="cellTitle(r, i)"
          >
            <template v-if="icpc">
              <template v-if="c.solved">
                <div class="top">{{ c.time_min }}</div>
                <div class="bot">{{ tries(c.attempts) }}</div>
              </template>
              <template v-else-if="c.pending">
                <div class="top">&nbsp;</div>
                <div class="bot">{{ t("board.pending", { a: c.attempts - c.pending, p: c.pending }) }}</div>
              </template>
              <template v-else-if="c.attempts">
                <div class="top">&nbsp;</div>
                <div class="bot">{{ tries(c.attempts) }}</div>
              </template>
            </template>
            <template v-else-if="c.score !== null || c.pending">
              <div class="top">{{ c.score !== null ? fmtScore(c.score) : "?" }}</div>
              <div class="bot">{{ c.pending ? t("board.pending", { a: c.attempts - c.pending, p: c.pending }) : tries(c.attempts) }}</div>
            </template>
          </td>
        </tr>
      </TransitionGroup>
      <tfoot v-if="!compact && board.rows.length && !board.hidden">
        <tr class="summary">
          <td />
          <td class="team">{{ t("board.summary") }}</td>
          <td :colspan="icpc ? 2 : 1" />
          <td v-for="(s, i) in summary" :key="i" class="cell sum">
            <template v-if="icpc">
              <div class="top">{{ s.solved }}</div>
              <div class="bot">{{ s.tried }}</div>
            </template>
            <div v-else class="top">{{ fmtScore(Math.round(s.avg * 10) / 10) }}</div>
          </td>
        </tr>
      </tfoot>
    </table>
    <div v-if="!compact && icpc" class="legend">
      <span class="lg first">{{ t("board.solvedFirst") }}</span>
      <span class="lg solved">{{ t("board.solved") }}</span>
      <span class="lg wrong">{{ t("board.tried") }}</span>
      <span class="lg pending">{{ t("board.triedPending") }}</span>
      <span class="lg untried">{{ t("board.untried") }}</span>
    </div>
  </div>
</template>

<style scoped>
.wrap {
  overflow-x: auto;
}
.sb {
  border-collapse: collapse;
  margin: 0 auto;
  font-size: 13.5px;
}
th {
  font-size: 11.5px;
  font-weight: 700;
  color: var(--text2);
  padding: 6px 4px;
  border-bottom: 2px solid var(--divider);
  letter-spacing: 0.03em;
}
td {
  border-bottom: 1px solid var(--divider);
  padding: 2px 4px;
}
tbody tr {
  background: var(--surface);
}
tbody tr.me td:not(.cell) {
  background: var(--sb-me);
}
.rank {
  width: 44px;
  text-align: center;
  font-weight: 600;
}
.rank.gold {
  background: var(--medal-gold) !important;
}
.rank.silver {
  background: var(--medal-silver) !important;
}
.rank.bronze {
  background: var(--medal-bronze) !important;
}
.team {
  text-align: right;
  min-width: 180px;
  padding-right: 10px;
}
.name {
  font-weight: 700;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 340px;
  margin-left: auto;
}
.aff {
  font-size: 11.5px;
  color: var(--text3);
}
.score {
  text-align: center;
}
.solved-n {
  width: 36px;
  text-align: center;
  font-weight: 700;
  font-size: 15px;
}
.time {
  width: 50px;
  text-align: center;
  color: var(--text2);
}
.prob {
  width: 66px;
  text-align: center;
}
.badge {
  display: inline-block;
  min-width: 26px;
  padding: 3px 4px;
  border-radius: 4px;
  font-weight: 700;
  font-size: 13px;
  background: var(--accent-soft);
  border: 1px solid rgba(0, 0, 0, 0.2);
  box-sizing: border-box;
}
.cell {
  width: 66px;
  height: 38px;
  text-align: center;
  border-left: 1px solid var(--app);
  line-height: 1.15;
  padding: 2px;
}
.cell .top {
  font-size: 13px;
  font-weight: 600;
}
.cell .bot {
  font-size: 10.5px;
  opacity: 0.9;
}
.cell.solved,
.lg.solved {
  background: var(--sb-solved);
  color: var(--sb-solved-ink);
}
.cell.partial {
  color: var(--sb-solved-ink);
}
.cell.first,
.lg.first {
  background: var(--sb-first);
  color: var(--sb-first-ink);
}
.cell.wrong,
.lg.wrong {
  background: var(--sb-wrong);
  color: var(--sb-wrong-ink);
}
.cell.pending,
.lg.pending {
  background: var(--sb-pending);
  color: var(--sb-pending-ink);
}
.cell.focus {
  outline: 3px solid #f2c94c;
  outline-offset: -3px;
}
.summary td {
  background: transparent;
  color: var(--text2);
  font-size: 12px;
}
.summary .team {
  font-weight: 600;
}
.cell.sum {
  border-left: 0;
}
.legend {
  display: flex;
  gap: 6px;
  justify-content: center;
  flex-wrap: wrap;
  margin-top: 14px;
  font-size: 12px;
}
.lg {
  padding: 3px 10px;
  border-radius: 3px;
}
.lg.untried {
  background: var(--surface);
  border: 1px solid var(--border);
}
.large .sb {
  font-size: 17px;
}
.large .cell {
  width: 82px;
  height: 50px;
}
.large .cell .top {
  font-size: 17px;
}
.large .cell .bot {
  font-size: 12px;
}
.large .name {
  max-width: 520px;
}
.row-move {
  transition: transform 0.9s ease;
}
@media (max-width: 760px) {
  .team {
    min-width: 110px;
  }
  .aff {
    display: none;
  }
  .cell,
  .prob {
    width: 46px;
  }
}
</style>
