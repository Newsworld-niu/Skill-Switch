import s from "../SettingsPage.module.css";

/** Accessible on/off switch used across settings rows. */
export function Toggle({
  checked,
  onChange,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <button
      type="button"
      className={`${s.toggle} ${checked ? s.toggleOn : ""}`}
      onClick={() => onChange(!checked)}
      role="switch"
      aria-checked={checked}
    >
      <span className={`${s.toggleThumb} ${checked ? s.toggleThumbOn : ""}`} />
    </button>
  );
}
