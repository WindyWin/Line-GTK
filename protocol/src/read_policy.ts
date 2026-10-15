export type ReadPermission =
  | "allowed"
  | "paused"
  | "auto_disabled"
  | "unavailable";

// UIの判定だけに頼らず、既読送信の直前に保存済み設定を確認する。
export async function readReceiptPermission(
  path: string,
): Promise<ReadPermission> {
  try {
    const settings = JSON.parse(await Deno.readTextFile(path));
    if (!settings || typeof settings !== "object" || Array.isArray(settings)) {
      return "unavailable";
    }
    if (
      (settings.pause_read_receipts !== undefined &&
        typeof settings.pause_read_receipts !== "boolean") ||
      (settings.auto_mark_read !== undefined &&
        typeof settings.auto_mark_read !== "boolean")
    ) return "unavailable";
    if (settings.pause_read_receipts === true) return "paused";
    if (settings.auto_mark_read === false) return "auto_disabled";
    return "allowed";
  } catch {
    // 設定を確認できない場合は、既読を送らない側へ倒す。
    return "unavailable";
  }
}
