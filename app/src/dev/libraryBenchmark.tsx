/** Opt-in Vite fixture; not imported by the application entry point. */
import { render } from "solid-js/web";
import { createSignal } from "solid-js";
import LibraryScreen from "../components/LibraryScreen";
import { loadDdragonStatus, loadGames } from "../api";
import type { HistoryState } from "../historyController";
import type { HistoryStats } from "../leagueIndex";
import "../styles/global.css";

if (!import.meta.env.DEV) throw new Error("Library fixture requires the development server");
if (new URLSearchParams(location.search).has("baseline")) {
  const style = document.createElement("style");
  style.textContent = "article { content-visibility: visible !important; }";
  document.head.append(style);
}

const source = await loadGames();
const assets = await loadDdragonStatus();
const games = Array.from({ length: 50 }, (_, index) => ({
  ...source[index % source.length],
  timestamp: String(1800000000 + index),
  media_id: `9c82b929-7c85-4c27-a6d0-${String(index).padStart(12, "0")}` as typeof source[number]["media_id"],
  incomplete: false,
  participants: source[0].participants,
  items: source[0].items,
  summoner_spells: source[0].summoner_spells,
  keystone_id: source[0].keystone_id,
}));
const history: HistoryState = {
  loading: false, indexing: false, initializing: false, error: null, indexIssue: null, laterPage: false,
  page: {
    next: null, indexed_recordings: 50, catalog_recordings: 50, unknown_recordings: 0,
    rows: games.map(game => ({
      game, media_id: game.media_id!, index_status: "ready",
      stats: {
        source: "final_eog", kills: game.kills, deaths: game.deaths, assists: game.assists,
        items: game.items.map(item => item.item_id), summoner_spells: game.summoner_spells,
        keystone_id: game.keystone_id, secondary_style: 8000, creep_score: 180,
        duration_seconds: game.duration_ms / 1000, evidence: null, final_evidence: null, game_tick: null,
      } satisfies HistoryStats,
    })),
  },
};
const host = document.getElementById("root")!;
host.style.cssText = "max-width:1120px;margin:24px auto;padding:0 24px";
const record = (action: string) => host.dataset.action = action;
const [currentHistory, setCurrentHistory] = createSignal(history);
performance.mark("library-mount-start");
render(() => <LibraryScreen tab="games" games={games} clips={[]} ddragon={assets}
  history={currentHistory()} actionable={true} busyId={null} snapshotOrigin={null}
  onHistoryQuery={query => setCurrentHistory({ ...history, page: { ...history.page!,
    rows: history.page!.rows.filter(row =>
      (!query.search || row.game.champion.toLowerCase().includes(query.search.toLowerCase()))
      && (!query.champion || row.game.champion === query.champion)
      && (!query.saved_only || row.game.saved)),
  } })}
  onOpenGame={id => record(`open:${id}`)} onToggleSaved={game => record(`save:${game.timestamp}`)}
  onDeleteGame={game => record(`delete:${game.timestamp}`)} onOpenClip={() => {}}
  onDeleteClip={() => {}} onOpenClipsFolder={() => {}} />, host);
requestAnimationFrame(() => requestAnimationFrame(() => {
  performance.mark("library-mounted");
  performance.measure("library-mount", "library-mount-start", "library-mounted");
  host.dataset.ready = "true";
}));
