import { batch, createEffect, createMemo, on, onCleanup, type Accessor } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import { libraryError } from "./libraryController";
import { readClipGridPage, replaceTagDefinition, type ClipGridCard, type ClipGridCursor, type ClipGridQuery, type ClipTagUpdate, type Tag } from "./managedClips";

type GridSource = { root: string | null; token: string; ready: boolean; query: ClipGridQuery };
type Operation = "refresh" | "append";

const multikillTiers = new Map([
  ["builtin:double_kill", 2], ["builtin:triple_kill", 3],
  ["builtin:quadra_kill", 4], ["builtin:pentakill", 5],
]);

function matchesTagFilters(tags: ClipGridCard["tags"], filters: string[]) {
  return filters.every(id => {
    const tier = multikillTiers.get(id);
    return tags.some(tag => tag.id === id || (tier !== undefined && tag.tier !== null && tag.tier >= tier));
  });
}

export function createClipGrid(source: Accessor<GridSource>) {
  const [state, setState] = createStore({
    clips: [] as ClipGridCard[], next: null as ClipGridCursor | null,
    token: undefined as string | undefined, busy: false,
    failure: undefined as { message: string; operation: Operation } | undefined,
  });
  let depth = 1;
  let generation = 0;
  let read = Promise.resolve();

  const current = createMemo(() => {
    const { ready, token } = source();
    return ready && state.token === token;
  });

  const readPage = async (request: number, token: string, query: ClipGridQuery, cursor: ClipGridCursor | null) => {
    for (let retries = 0; request === generation; retries++) {
      try {
        return await readClipGridPage(token, query, cursor);
      } catch (error) {
        // The native index has a fixed number of concurrent read slots.
        if (retries === 2 || !error || typeof error !== "object" || !("code" in error) || error.code !== "busy") throw error;
        await new Promise<void>(resolve => window.setTimeout(resolve, 150));
      }
    }
  };

  const load = (operation: Operation) => {
    const { ready, token, query } = source();
    const append = operation === "append";
    const start = append ? state.next : null;
    if (!ready || state.busy || (append && (!current() || !start))) return Promise.resolve(undefined);

    const request = generation;
    const pages = append ? 1 : depth;
    setState("busy", true);
    // Finish the current read before issuing one for a superseding catalog or filter.
    const result = read.then(async () => {
      if (request !== generation) return;
      try {
        const clips: ClipGridCard[] = [];
        let cursor = start;
        for (let i = 0; i < pages; i++) {
          const page = await readPage(request, token, query, cursor);
          if (!page || request !== generation) return;
          clips.push(...page.clips);
          cursor = page.next;
          if (!cursor) break;
        }
        const firstNew = state.clips.length;
        batch(() => {
          if (append) {
            for (const [index, clip] of clips.entries()) setState("clips", firstNew + index, clip);
            depth++;
          } else {
            // Reconcile the full loaded range together, using fresh cursors for this catalog.
            setState("clips", reconcile(clips, { key: "id" }));
          }
          setState({ next: cursor, token, failure: undefined });
        });
        if (append && clips.length) return firstNew;
      } catch (error) {
        if (request === generation) setState("failure", { message: libraryError(error), operation });
      } finally {
        if (request === generation) setState("busy", false);
      }
    });
    read = result.then(() => {});
    return result;
  };

  createEffect(on(source, (next, previous) => {
    // A catalog token grants read access. The root and filters define the browse session.
    const newLibrary = !previous || next.root !== previous.root;
    const newSession = newLibrary || next.query !== previous?.query;
    if (!newSession && next.token === previous.token && next.ready === previous.ready) return;
    generation++;
    batch(() => {
      if (newSession) {
        depth = 1;
        setState({ next: null, token: undefined });
        if (newLibrary) setState("clips", []);
      }
      setState({ busy: false, failure: undefined });
      if (next.ready && !current()) void load("refresh");
    });
  }));
  onCleanup(() => { generation++; });

  const refreshTagResults = () => {
    if (!source().ready) return;
    // A tag-dependent query may lose or gain the item. Re-seek only the
    // loaded range, keeping stable card identities until the result is ready.
    generation++;
    setState({ busy: false, failure: undefined });
    void load("refresh");
  };
  const tagsEdited = ({ id, tags }: ClipTagUpdate) => {
    const index = state.clips.findIndex(clip => clip.id === id);
    const { query } = source();
    const previous = state.clips[index]?.tags;
    let tagMembershipChanged = false;
    if (query.tags?.length) {
      const matchedBefore = previous ? matchesTagFilters(previous, query.tags) : false;
      tagMembershipChanged = matchedBefore !== matchesTagFilters(tags, query.tags);
    }
    const labelsMayChange = query.search && (!previous ||
      previous.some(tag => !tags.some(updated => updated.label === tag.label)) ||
      tags.some(tag => !previous.some(old => old.label === tag.label)));
    if (index >= 0) setState("clips", index, "tags", tags);
    if (tagMembershipChanged || labelsMayChange || state.busy) refreshTagResults();
  };
  const tagRenamed = (tag: Tag) => {
    batch(() => {
      for (const [index, clip] of state.clips.entries()) {
        const tags = replaceTagDefinition(clip.tags, tag);
        if (tags !== clip.tags) setState("clips", index, "tags", tags);
      }
    });
    if (source().query.search || state.busy) refreshTagResults();
  };
  return { state, current, tagsEdited, tagRenamed, append: () => load("append"), retry: () => load(state.failure?.operation ?? "refresh") };
}
