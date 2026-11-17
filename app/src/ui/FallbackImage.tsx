import { Show, createSignal, type JSX } from "solid-js";

type Props = {
  src?: string | null;
  fallback: JSX.Element;
  class?: string;
  style?: JSX.CSSProperties;
  loading?: "lazy" | "eager";
};

function LoadingImage(props: Props & { src: string }) {
  const [state, setState] = createSignal<"loading" | "loaded" | "failed">("loading");
  return <>
    <Show when={state() !== "loaded"}>{props.fallback}</Show>
    <Show when={state() !== "failed"}>
      <img
        src={props.src}
        class={props.class}
        alt=""
        draggable={false}
        loading={props.loading}
        style={{ ...props.style, visibility: state() === "loaded" ? undefined : "hidden",
          position: state() === "loaded" ? undefined : "absolute" }}
        onLoad={() => setState("loaded")}
        onError={() => setState("failed")}
      />
    </Show>
  </>;
}

/** Decorative artwork whose placeholder stays in view until the image loads. */
export default function FallbackImage(props: Props) {
  return <Show when={props.src} keyed fallback={props.fallback}>
    {src => <LoadingImage {...props} src={src} />}
  </Show>;
}
