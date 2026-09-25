import { ArrowLeftIcon } from "./icons";
import styles from "./controls.module.css";

type Props = {
  label: string;
  onClick: () => void;
  disabled?: boolean;
  class?: string;
};

export default function BackButton(props: Props) {
  const className = () =>
    [styles.control, props.class]
      .filter((value): value is string => Boolean(value))
      .join(" ");

  return (
    <button
      class={className()}
      type="button"
      disabled={props.disabled}
      onClick={props.onClick}
    >
      <ArrowLeftIcon size={16} />
      <span>{props.label}</span>
    </button>
  );
}