export function memory(mb: number | null | undefined): string {
  if (mb === null || mb === undefined) return "";
  if (mb >= 1000) return `${(mb / 1024).toFixed(1)} GB`;
  return `${Math.round(mb)} MB`;
}

export function cpu(percent: number | null | undefined): string {
  if (percent === null || percent === undefined) return "";
  return `${percent < 10 ? percent.toFixed(1) : Math.round(percent)}% CPU`;
}
