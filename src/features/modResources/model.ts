import type { TaskSnapshot } from "../tasks/types";
import type { InstalledModResource, ModResourceAsset, ModResourceState } from "./types";

export function resourceTaskAsset(task: TaskSnapshot): string | undefined {
  return task.subject?.split(":")[1];
}

export function resourceTaskMatches(task: TaskSnapshot, edition: string): boolean {
  return task.kind === "mod-resource-install"
    && task.subject?.startsWith(`${edition}:`) === true;
}

/** Latest result per resource survives leaving and reopening its page. */
export function latestResourceTasks(tasks: readonly TaskSnapshot[], edition: string): Map<string, TaskSnapshot> {
  const latest = new Map<string, TaskSnapshot>();
  for (const task of tasks) {
    const asset = resourceTaskAsset(task);
    if (!asset || !resourceTaskMatches(task, edition)) continue;
    const previous = latest.get(asset);
    if (!previous || task.task_id > previous.task_id) latest.set(asset, task);
  }
  return latest;
}

export function resourceDownloadUrl(asset: ModResourceAsset, source?: string): string {
  return asset.mirrors?.find(mirror => mirror.platform === source)?.url
    ?? asset.mirrors?.[0]?.url ?? asset.url;
}

export function resourceInstallLocation(asset: ModResourceAsset, state: ModResourceState): string {
  return state.mods_directory ? `${state.mods_directory}\\${asset.id}` : "";
}

const REASON_COPY: Record<string, readonly [string, string]> = {
  replace_available: ["已安装；下载将覆盖同名 Mod。", "Installed. Downloading replaces the Mod with the same name."],
  processed: ["此 Mod 已加工，保留现有内容。", "This Mod has custom features. Its files will be preserved."],
  unknown_source: ["无法确认此目录的来源，保留现有内容。", "The origin of this directory is unknown. Its files will be preserved."],
  invalid_receipt: ["安装记录无效，保留现有内容。", "The installation record is invalid. Existing files will be preserved."],
  locally_modified: ["本地文件已修改，保留现有 Mod。请先另存修改，再安装标准成品。", "Local files have changed. Save your customized Mod separately before installing the standard package."],
  integrity_unavailable: ["无法校验本地文件，暂不覆盖现有 Mod。", "Local files could not be verified. This Mod will not be overwritten."],
  legacy_unverified: ["旧版 Mod 将在安装前校验，只有原始文件才会被接管。", "This legacy Mod will be verified before replacement. Only an unmodified package can be adopted."],
  update_available: ["有可用更新。请关闭游戏后更新。", "An update is available. Close the game before updating."],
  integrity_unchecked: ["尚未检查本地完整性，可点击“检查更新”进行验证。", "Local integrity has not been checked. Choose Check updates to verify it."],
  current: ["本地文件已校验。", "Local files verified."],
  not_installed: ["", ""],
};

export function resourceStatusMessage(status: InstalledModResource | undefined, en: boolean): string {
  if (!status) return "";
  const known = status.reason_code ? REASON_COPY[status.reason_code] : undefined;
  if (known) return known[en ? 1 : 0];
  if (en && status.update_available) return REASON_COPY.update_available[1];
  return status.message;
}
