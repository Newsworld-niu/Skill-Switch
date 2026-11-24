import { useEffect } from "react";

function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return (
    target.isContentEditable ||
    ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName)
  );
}

/**
 * ↑/↓ moves the selection through `ids` when focus is not in a text field and
 * no modal (other than the detail drawer) is open. The newly selected card is scrolled into view if it carries
 * `data-list-id={id}`.
 */
export function useListKeyboardNav(
  ids: readonly string[],
  selectedId: string | null,
  onSelect: (id: string) => void
) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
      if (event.defaultPrevented || event.metaKey || event.ctrlKey) return;
      if (isTypingTarget(event.target) || ids.length === 0) return;
      // The detail drawer is not a blocking modal: ↑/↓ keeps browsing behind it.
      if (document.querySelector("[data-modal-overlay]:not([data-drawer])"))
        return;

      event.preventDefault();
      const current = selectedId ? ids.indexOf(selectedId) : -1;
      const delta = event.key === "ArrowDown" ? 1 : -1;
      const next =
        current === -1
          ? 0
          : Math.min(ids.length - 1, Math.max(0, current + delta));
      const nextId = ids[next];
      onSelect(nextId);
      document
        .querySelector(`[data-list-id="${CSS.escape(nextId)}"]`)
        ?.scrollIntoView({ block: "nearest" });
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [ids, selectedId, onSelect]);
}
