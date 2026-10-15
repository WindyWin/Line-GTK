import { assertEquals } from "@std/assert";
import { readReceiptPermission } from "../src/read_policy.ts";

Deno.test("既読停止と従来の自動既読設定を、毎回保存済みファイルから確認する", async () => {
  const directory = await Deno.makeTempDir();
  const path = `${directory}/settings.json`;
  try {
    assertEquals(await readReceiptPermission(path), "unavailable");
    await Deno.writeTextFile(path, JSON.stringify({ auto_mark_read: true }));
    assertEquals(await readReceiptPermission(path), "allowed");
    await Deno.writeTextFile(
      path,
      JSON.stringify({ auto_mark_read: true, pause_read_receipts: true }),
    );
    assertEquals(await readReceiptPermission(path), "paused");
    await Deno.writeTextFile(
      path,
      JSON.stringify({ auto_mark_read: false, pause_read_receipts: false }),
    );
    assertEquals(await readReceiptPermission(path), "auto_disabled");
    await Deno.writeTextFile(
      path,
      JSON.stringify({ auto_mark_read: true, pause_read_receipts: false }),
    );
    assertEquals(await readReceiptPermission(path), "allowed");
  } finally {
    await Deno.remove(directory, { recursive: true });
  }
});

Deno.test("設定が壊れていたり型が違ったりする場合は既読を送らない", async () => {
  const directory = await Deno.makeTempDir();
  const path = `${directory}/settings.json`;
  try {
    for (
      const contents of [
        "{",
        "null",
        "[]",
        '{"pause_read_receipts":"false"}',
        '{"auto_mark_read":"true"}',
      ]
    ) {
      await Deno.writeTextFile(path, contents);
      assertEquals(await readReceiptPermission(path), "unavailable");
    }
  } finally {
    await Deno.remove(directory, { recursive: true });
  }
});
