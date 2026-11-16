import { createEffect, createSignal, onCleanup, onMount } from "solid-js";

export function createPlaybackHud(canAutoHide: () => boolean) {
  let idleTimer: number | undefined;
  let overlay: HTMLElement | undefined;
  let heldPointer: number | undefined;

  const [topBarVisible, setTopBarVisible] = createSignal(true);
  const [hovered, setHovered] = createSignal(false);
  const [focused, setFocused] = createSignal(false);
  const [pressed, setPressed] = createSignal(false);
  const canHide = () => canAutoHide()
    && !hovered() && !focused() && !pressed();

  const noteActivity = () => {
    setTopBarVisible(true);
    if (idleTimer !== undefined) window.clearTimeout(idleTimer);
    idleTimer = undefined;
    if (canHide()) idleTimer = window.setTimeout(() => {
      idleTimer = undefined;
      if (canHide()) setTopBarVisible(false);
    }, 3_200);
  };
  createEffect(noteActivity);
  const inHud = (target: EventTarget | null) => target instanceof Element
    && !!overlay?.contains(target) && !!target.closest("[data-hud-region]");
  const pointerMove = (event: PointerEvent) => {
    // Fullscreen can move the HUD away without a pointerleave event.
    setHovered(inHud(event.target));
    noteActivity();
  };
  const pointerDown = (event: PointerEvent) => {
    if (inHud(event.target)) { heldPointer = event.pointerId; setPressed(true); }
    noteActivity();
  };
  const pointerUp = (event: PointerEvent) => {
    if (pressed() && event.pointerId !== heldPointer) return;
    heldPointer = undefined;
    setPressed(false);
    noteActivity();
  };
  const windowBlur = () => {
    heldPointer = undefined;
    setPressed(false); setHovered(false); setFocused(false);
    noteActivity();
  };
  const windowFocus = () => {
    setFocused(inHud(document.activeElement));
    noteActivity();
  };

  onMount(() => {
    // Observe the video's events without adding a surface above it.
    window.addEventListener("pointermove", pointerMove, {
      passive: true,
      capture: true,
    });
    window.addEventListener("pointerdown", pointerDown, {
      passive: true,
      capture: true,
    });
    window.addEventListener("pointerup", pointerUp, true);
    window.addEventListener("pointercancel", pointerUp, true);
    window.addEventListener("lostpointercapture", pointerUp, true);
    window.addEventListener("blur", windowBlur);
    window.addEventListener("focus", windowFocus);
    window.addEventListener("keydown", noteActivity, true);

    onCleanup(() => {
      window.removeEventListener("pointermove", pointerMove, true);
      window.removeEventListener("pointerdown", pointerDown, true);
      window.removeEventListener("pointerup", pointerUp, true);
      window.removeEventListener("pointercancel", pointerUp, true);
      window.removeEventListener("lostpointercapture", pointerUp, true);
      window.removeEventListener("blur", windowBlur);
      window.removeEventListener("focus", windowFocus);
      window.removeEventListener("keydown", noteActivity, true);
      if (idleTimer !== undefined) window.clearTimeout(idleTimer);
    });
  });

  return {
    visible: topBarVisible,
    setRoot: (element: HTMLElement) => { overlay = element; },
    regionEvents: {
      onPointerEnter: () => setHovered(true),
      onPointerLeave: () => setHovered(false),
      onFocusIn: () => setFocused(true),
      onFocusOut: (event: FocusEvent) => setFocused(inHud(event.relatedTarget)),
    },
  };
}
