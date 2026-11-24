import type { Provenance } from "../types";

/** Icon colors derived from an item name (stable per first character). */
export function getIconColors(name: string): { bg: string; fg: string } {
  const palettes = [
    { bg: "#6366f1", fg: "#ffffff" },
    { bg: "#22c55e", fg: "#ffffff" },
    { bg: "#ef4444", fg: "#ffffff" },
    { bg: "#06b6d4", fg: "#ffffff" },
    { bg: "#f97316", fg: "#ffffff" },
    { bg: "#ec4899", fg: "#ffffff" },
    { bg: "#8b5cf6", fg: "#ffffff" },
    { bg: "#0ea5e9", fg: "#ffffff" },
  ];
  return palettes[name.charCodeAt(0) % palettes.length];
}

// Format timestamp to readable date
export function formatDate(ts: number): string {
  return new Date(ts).toLocaleDateString("zh-CN");
}

// Get provenance badge label
export function getProvenanceBadge(provenance?: Provenance): string {
  if (!provenance) return "";
  switch (provenance.kind) {
    case "manual":
      return "";
    case "file-import":
      return "导入";
    case "external-app":
      return provenance.label || "外部导入";
    case "marketplace":
      return "市场导入";
    case "repo-source":
      return provenance.sourceName
        ? `仓库源 · ${provenance.sourceName}`
        : "仓库源导入";
    case "plugin":
      return provenance.sourceName
        ? `插件 · ${provenance.sourceName}`
        : provenance.label || "插件投射";
    default:
      return "";
  }
}
