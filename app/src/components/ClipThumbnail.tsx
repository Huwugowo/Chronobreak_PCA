import FallbackImage from "../ui/FallbackImage";
import styles from "./Library.module.css";

export default function ClipThumbnail(props: { src?: string | null }) {
  return <FallbackImage src={props.src} loading="lazy" fallback={
    <span class={styles.clipPlaceholder} aria-hidden="true">No thumbnail</span>
  } />;
}
