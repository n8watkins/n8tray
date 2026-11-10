export function memory(mb: number | null | undefined): string {
  if (mb === null || mb === undefined) return "";
  if (mb >= 1000) return `${(mb / 1024).toFixed(1)} GB`;
  return `${Math.round(mb)} MB`;
}

export function cpu(percent: number | null | undefined): string {
  if (percent === null || percent === undefined) return "";
  return `${percent < 10 ? percent.toFixed(1).replace(/\.0$/, "") : Math.round(percent)}% CPU`;
}

/** A percentage without the word, for tables. */
export function percent(value: number | null | undefined): string {
  if (value === null || value === undefined) return "-";
  return `${value < 10 ? value.toFixed(1).replace(/\.0$/, "") : Math.round(value)}%`;
}
