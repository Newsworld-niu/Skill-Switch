import { useEffect } from "react";

interface Handlers {
  /** ⌘/Ctrl+N */
  onNew: () => void;
  /** ⌘/Ctrl+, */
  onSettings: () => void;
}

function isVisible(el: HTMLElement): boolean {
  return el.offsetParent !== null || el.getClientRects().length > 0;
}

/**
 * App-wide keyboard shortcuts:
 * - ⌘/Ctrl+K: focus the current page's search box (`[data-search-input]`)
 * - ⌘/Ctrl+N: create a new skill
 * - ⌘/Ctrl+,: open settings
 * - Esc: close the topmost modal (`[data-modal-overlay]`, closed via its own click handler)
 */
export function useGlobalShortcuts({ onNew, onSettings }: Handlers) {
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.isComposing) return;

      if (event.key === "Escape") {
        const overlays = document.querySelectorAll<HTMLElement>(
          "[data-modal-overlay]"
        );
        const top = overlays[overlays.length - 1];
        if (top) {
          event.preventDefault();
          top.click();
        }
        return;
      }

      const mod = event.metaKey || event.ctrlKey;
      if (!mod || event.altKey || event.shiftKey) return;
      // Leave shortcuts alone while a modal is open.
      if (document.querySelector("[data-modal-overlay]")) return;

      const key = event.key.toLowerCase();
      if (key === "k") {
        const input = Array.from(
          document.querySelectorAll<HTMLInputElement>("[data-search-input]")
        ).find(isVisible);
        if (input) {
          event.preventDefault();
          input.focus();
          input.select();
        }
      } else if (key === "n") {
        event.preventDefault();
        onNew();
      } else if (key === ",") {
        event.preventDefault();
        onSettings();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onNew, onSettings]);
}
