import { onCleanup, onMount } from "solid-js";

const openDialogs: HTMLElement[] = [];
const topDialog = () => openDialogs[openDialogs.length - 1];
const focusable = "button, input, select, textarea, a[href], summary, [tabindex], [contenteditable=true]";

function tabStops(dialog: HTMLElement) {
  return [...dialog.querySelectorAll<HTMLElement>(focusable)]
    .filter(element => {
      if (element.tabIndex < 0 || element.matches(":disabled") || element.closest("[inert], [hidden]")) return false;
      const collapsed = element.closest("details:not([open])");
      if (collapsed && !collapsed.querySelector(":scope > summary")?.contains(element)) return false;
      for (let node: HTMLElement | null = element; node; node = node.parentElement) {
        const style = getComputedStyle(node);
        if (style.display === "none" || style.visibility === "hidden" || style.visibility === "collapse") return false;
        if (node === dialog) break;
      }
      return true;
    })
    .sort((a, b) => (a.tabIndex || Number.MAX_SAFE_INTEGER) - (b.tabIndex || Number.MAX_SAFE_INTEGER));
}

export function createModalFocus(options: {
  dialog: () => HTMLElement;
  initial: () => HTMLElement;
  onEscape: () => void;
  opener?: () => HTMLElement | undefined;
}) {
  onMount(() => {
    const dialog = options.dialog();
    const opener = options.opener?.() ?? document.activeElement;
    const isTop = () => topDialog() === dialog;
    const focusFirst = () => (tabStops(dialog)[0] ?? dialog).focus();
    openDialogs.push(dialog);

    const onFocus = (event: FocusEvent) => {
      if (isTop() && event.target instanceof Node && !dialog.contains(event.target)) focusFirst();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (!isTop() || event.defaultPrevented) return;
      if (event.key === "Escape") {
        event.preventDefault();
        if (!event.repeat) options.onEscape();
      } else if (event.key === "Tab") {
        const stops = tabStops(dialog);
        const active = document.activeElement;
        const edge = event.shiftKey ? stops[0] : stops[stops.length - 1];
        if (active === edge || !stops.some(element => element === active)) {
          event.preventDefault();
          ((event.shiftKey ? stops[stops.length - 1] : stops[0]) ?? dialog).focus();
        }
      }
    };

    document.addEventListener("focusin", onFocus);
    window.addEventListener("keydown", onKeyDown);
    options.initial().focus();
    if (!dialog.contains(document.activeElement)) focusFirst();

    onCleanup(() => {
      openDialogs.splice(openDialogs.lastIndexOf(dialog), 1);
      document.removeEventListener("focusin", onFocus);
      window.removeEventListener("keydown", onKeyDown);
      queueMicrotask(() => {
        const activeDialog = topDialog();
        if (opener instanceof HTMLElement && opener.isConnected && !opener.closest("[hidden]") &&
            (!activeDialog || activeDialog.contains(opener))) opener.focus();
        if (activeDialog && !activeDialog.contains(document.activeElement)) activeDialog.focus();
      });
    });
  });
}
