// Arrow-key movement between the hub's rows and within a row (plan parity:
// Tab, Shift+Tab, arrows, Enter, Space, Escape). Tab order stays the
// document's; arrows are a faster way through the list.

/** The next index for a key in a list of `count`, or null for other keys. */
export function nextIndex(key: string, current: number, count: number): number | null {
  if (count <= 0) return null;
  switch (key) {
    case "ArrowDown":
      return current < 0 ? 0 : Math.min(count - 1, current + 1);
    case "ArrowUp":
      return current < 0 ? count - 1 : Math.max(0, current - 1);
    case "Home":
      return 0;
    case "End":
      return count - 1;
    default:
      return null;
  }
}

/**
 * Moves focus between rows: Up and Down go to the same part (name or
 * action) of the row above or below, Left and Right between the parts of one
 * row. Elements carry `data-row` (the row's index) and `data-part`.
 */
export function moveFocus(event: KeyboardEvent, root: HTMLElement): boolean {
  const target = event.target as HTMLElement | null;
  if (!target || target.dataset.row === undefined) return false;
  const row = Number(target.dataset.row);
  const part = target.dataset.part ?? "main";
  const all = Array.from(root.querySelectorAll<HTMLElement>("[data-row]:not(:disabled)"));
  const rows = Array.from(new Set(all.map((e) => Number(e.dataset.row)))).sort((a, b) => a - b);
  const inRow = (r: number) => all.filter((e) => Number(e.dataset.row) === r);
  if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
    const parts = inRow(row);
    const i = parts.indexOf(target);
    const j = event.key === "ArrowRight" ? i + 1 : i - 1;
    if (j < 0 || j >= parts.length) return false;
    parts[j].focus();
    return true;
  }
  const at = rows.indexOf(row);
  const next = nextIndex(event.key, at, rows.length);
  if (next === null || next === at) return next !== null;
  const parts = inRow(rows[next]);
  (parts.find((e) => (e.dataset.part ?? "main") === part) ?? parts[0])?.focus();
  return true;
}
