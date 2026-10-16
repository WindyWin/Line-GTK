import { assert, assertEquals } from "@std/assert";
import type { Client } from "@evex/linejs";
import { chatEventMessage, createChatEvents } from "../src/chat_events.ts";

const invitation = {
  type: 124,
  revision: "9007199254740993",
  createdTime: 1_760_000_000_000,
  param1: "c-group",
  param2: "u-alice",
  param3: "u-bob\x1eu-charlie\x1eu-bob",
};
Deno.test("招待した人と複数の対象を分け、数値と名前付きの操作を同じイベントにする", () => {
  const message = chatEventMessage(invitation, "u-self")!;
  assertEquals(message.chatEvent, {
    kind: "invited",
    actorMid: "u-alice",
    targetMids: ["u-bob", "u-charlie"],
  });
  assertEquals(message.id, "event:9007199254740993");
  assertEquals(
    chatEventMessage(
      { ...invitation, type: "NOTIFIED_INVITE_INTO_CHAT" },
      "u-self",
    ),
    message,
  );
  const own = chatEventMessage({
    ...invitation,
    type: 123,
    param2: "u-bob\x1eu-charlie",
  }, "u-self")!;
  assertEquals(own.chatEvent.actorMid, "u-self");
  assertEquals(own.chatEvent.targetMids, ["u-bob", "u-charlie"]);
});
Deno.test("参加・退出・メンバー削除と旧グループ操作を区別する", () => {
  for (
    const [type, kind] of [
      [130, "joined"],
      [60, "joined"],
      [17, "joined"],
      [128, "left"],
      [61, "left"],
      [15, "left"],
      [133, "removed"],
      [19, "removed"],
    ] as const
  ) {
    const result = chatEventMessage({ ...invitation, type }, "u-self")!;
    assertEquals(result.chatEvent.kind, kind);
    assertEquals(result.chatEvent.actorMid, "u-alice");
    assertEquals(
      result.chatEvent.targetMids,
      kind === "removed" ? ["u-bob", "u-charlie"] : [],
    );
  }
  const seconds = chatEventMessage({
    ...invitation,
    type: 130,
    createdTime: 1_760_000_000,
  }, "u-self")!;
  assertEquals(seconds.createdTime, invitation.createdTime);
});
Deno.test("通常のメッセージ・不完全な操作・個別トーク・不正なIDはイベント化しない", () => {
  for (
    const patch of [{ type: 26 }, { param1: "u-friend" }, { param2: "" }, {
      param3: "",
    }, { param1: "c/../../other" }]
  ) assertEquals(chatEventMessage({ ...invitation, ...patch }, "u-self"), null);
  assertEquals(chatEventMessage(invitation, "u/../../other"), null);
});
function fixture(directory: string) {
  let mid = "u-self";
  let client: Client | null = {} as Client;
  const events: Record<string, unknown>[] = [];
  const profiles: Record<string, unknown>[][] = [];
  const runtime = {
    directory,
    getClient: () => client,
    selfMid: () => mid,
    observeProfiles: (messages: Record<string, unknown>[]) =>
      profiles.push(messages),
    emit: (event: string, payload: Record<string, unknown>) =>
      events.push({ event, ...payload }),
  };
  return {
    runtime,
    service: createChatEvents(runtime),
    events,
    profiles,
    switchAccount: () => {
      mid = "u-other";
      client = {} as Client;
    },
  };
}
Deno.test("重複受信は1件にし、保存・再起動・本文との時刻順統合・名前の照会に対応する", async () => {
  const directory = await Deno.makeTempDir();
  try {
    const f = fixture(directory);
    f.service.handleOperation(invitation);
    f.service.handleOperation(invitation);
    await f.service.settled();
    assertEquals(f.events.length, 1);
    assertEquals(f.profiles[0].map((m) => m.from), [
      "u-alice",
      "u-bob",
      "u-charlie",
    ]);
    const restored = createChatEvents(f.runtime);
    const merged = await restored.merge("c-group", [{
      id: "1",
      createdTime: invitation.createdTime - 1000,
      text: "前",
    }, { id: "2", createdTime: invitation.createdTime + 1000, text: "後" }]);
    assertEquals(merged.map((m) => m.id), ["1", "event:9007199254740993", "2"]);
    const cropped = await restored.merge("c-group", [{
      id: "3",
      createdTime: invitation.createdTime + 1000,
    }]);
    assertEquals(cropped.length, 1);
    assertEquals(await restored.merge("c-other", []), []);
    assert(
      (await Deno.readTextFile(directory + "/events-u-self.json")).includes(
        "u-bob",
      ),
    );
  } finally {
    await Deno.remove(directory, { recursive: true });
  }
});
Deno.test("別アカウントのイベントを混ぜず、ログイン変更前に届いた操作は保存しない", async () => {
  const directory = await Deno.makeTempDir();
  try {
    const f = fixture(directory);
    f.service.handleOperation(invitation);
    f.switchAccount();
    await f.service.settled();
    assertEquals(f.events, []);
    assertEquals(await f.service.merge("c-group", []), []);
  } finally {
    await Deno.remove(directory, { recursive: true });
  }
});

Deno.test("壊れたキャッシュのイベントを無視し、通常の本文を失わない", async () => {
  const directory = await Deno.makeTempDir();
  try {
    await Deno.writeTextFile(
      directory + "/events-u-self.json",
      JSON.stringify([{
        id: "event:1",
        to: "c-group",
        contentType: "CHAT_EVENT",
        createdTime: 1000,
        chatEvent: {
          kind: "invited",
          actorMid: "u-alice",
          targetMids: "壊れた対象一覧",
        },
      }]),
    );
    const f = fixture(directory);
    const messages = [{ id: "42", text: "本文", createdTime: 500 }];
    assertEquals(await f.service.merge("c-group", messages), messages);
  } finally {
    await Deno.remove(directory, { recursive: true });
  }
});
