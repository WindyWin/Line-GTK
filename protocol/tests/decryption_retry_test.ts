import { assertEquals } from "@std/assert";
import { type Client, TalkMessage } from "@evex/linejs";
import {
  createDecryptionRetry,
  type RetryClock,
} from "../src/decryption_retry.ts";
import { DECRYPTION_FAILED, guardedDecrypt } from "../src/e2ee_guard.ts";
import { createListener } from "../src/listener.ts";

type Json = Record<string, unknown>;
function fakeClock() {
  let now = 0;
  let sequence = 0;
  const tasks = new Map<number, { at: number; run: () => Promise<void> }>();
  const clock: RetryClock = {
    now: () => now,
    schedule: (run, delay) => {
      const id = ++sequence;
      tasks.set(id, { at: now + delay, run });
      return () => {
        tasks.delete(id);
      };
    },
  };
  return {
    clock,
    size: () => tasks.size,
    now: () => now,
    next: async () => {
      const entry = [...tasks.entries()].sort((a, b) => a[1].at - b[1].at)[0];
      if (!entry) throw new Error("No scheduled retry");
      tasks.delete(entry[0]);
      now = entry[1].at;
      await entry[1].run();
    },
    advance: (ms: number) => {
      now += ms;
    },
  };
}
function raw(id = "1", generation = 1): Json {
  return {
    id,
    from: "u-friend",
    to: "c-group",
    createdTime: Number(id),
    contentType: "NONE",
    text: "",
    contentMetadata: { e2eeVersion: "2", [DECRYPTION_FAILED]: "1" },
    chunks: [
      new Uint8Array(1),
      new Uint8Array(1),
      new Uint8Array(1),
      new Uint8Array([1]),
      new Uint8Array([generation]),
    ],
  };
}
function client() {
  return { base: { profile: { mid: "u-self" } } } as Client;
}

Deno.test("group retries keep both key generations and leave new messages unblocked", async () => {
  const timer = fakeClock();
  const owner = client();
  const recovered: string[] = [];
  const attempts: number[] = [];
  const available = new Set([2]);
  const retry = createDecryptionRetry({
    clock: timer.clock,
    getClient: () => owner,
    decrypt: (_client, message) => {
      assertEquals(
        (message.contentMetadata as Json)[DECRYPTION_FAILED],
        undefined,
      );
      const generation = (message.chunks as Uint8Array[])[4][0];
      attempts.push(generation);
      if (!available.has(generation)) throw new Error("Key not available yet");
      return Promise.resolve({ ...message, text: `generation ${generation}` });
    },
    recovered: (_client, message) => {
      recovered.push(String(message.id));
      return Promise.resolve();
    },
  });
  const first = raw("1", 1);
  retry.enqueue(owner, first);
  retry.enqueue(owner, first); // A duplicate must not extend the deadline.
  retry.enqueue(owner, raw("2", 2));
  assertEquals(attempts, []);
  assertEquals(timer.size(), 2);
  await timer.next(); // K1 still missing.
  await timer.next(); // K2 succeeds independently.
  assertEquals(recovered, ["2"]);
  available.add(1);
  await timer.next();
  assertEquals(recovered, ["2", "1"]);
  assertEquals(attempts, [1, 2, 1]);
  assertEquals((first.contentMetadata as Json)[DECRYPTION_FAILED], "1");
  assertEquals(timer.size(), 0);
});

Deno.test("permanent failures stop at three minutes without duplicate deadline extension", async () => {
  const timer = fakeClock();
  const owner = client();
  const times: number[] = [];
  const retry = createDecryptionRetry({
    clock: timer.clock,
    getClient: () => owner,
    decrypt: () => {
      times.push(timer.now());
      return Promise.resolve(raw()); // guardedDecrypt can return a flagged envelope.
    },
    recovered: () => {
      throw new Error("Unexpected recovery");
    },
  });
  retry.enqueue(owner, raw());
  while (timer.size()) {
    retry.enqueue(owner, raw());
    await timer.next();
  }
  assertEquals(times, [
    1000,
    4000,
    14000,
    44000,
    74000,
    104000,
    134000,
    164000,
  ]);
  assertEquals(timer.now(), 180000);
});

Deno.test("logout, client replacement, reset and expiry discard late retry results", async () => {
  for (const change of ["logout", "replace", "reset", "expire"] as const) {
    const timer = fakeClock();
    const owner = client();
    let current: Client | null = owner;
    let release!: (raw: Json) => void;
    const recovered: Json[] = [];
    const retry = createDecryptionRetry({
      clock: timer.clock,
      getClient: () => current,
      decrypt: () =>
        new Promise((resolve) => {
          release = resolve;
        }),
      recovered: (_client, message) => {
        recovered.push(message);
        return Promise.resolve();
      },
    });
    retry.enqueue(owner, raw());
    const pending = timer.next();
    if (change === "logout") current = null;
    if (change === "replace") current = client();
    if (change === "reset") retry.reset();
    if (change === "expire") timer.advance(180000);
    release({ ...raw(), contentMetadata: {}, text: "recovered" });
    await pending;
    assertEquals(recovered, []);
    assertEquals(timer.size(), 0);
  }
});

Deno.test("retry queue is bounded and excludes DMs and unencrypted messages", () => {
  const timer = fakeClock();
  const owner = client();
  const retry = createDecryptionRetry({
    clock: timer.clock,
    getClient: () => owner,
    decrypt: () => {
      throw new Error("Not yet due");
    },
    recovered: () => Promise.resolve(),
  });
  retry.enqueue(owner, { ...raw(), to: "u-friend" });
  retry.enqueue(owner, { ...raw(), chunks: [] });
  assertEquals(timer.size(), 0);
  for (let id = 1; id <= 140; id++) retry.enqueue(owner, raw(String(id)));
  assertEquals(timer.size(), 128);
  retry.reset();
  assertEquals(timer.size(), 0);
});

Deno.test("listener upgrades a failed ID once without duplicate unread or cursor changes", async () => {
  const timer = fakeClock();
  const owner = client();
  const handlers = new Map<string, (value: TalkMessage) => Promise<void>>();
  let listening = false;
  let available = false;
  const events: { event: string; payload?: Json }[] = [];
  const cursors: string[] = [];
  const observed: string[] = [];
  const cached: string[] = [];
  const upserts: string[] = [];
  Object.assign(owner.base, {
    e2ee: {
      decryptE2EEMessage: guardedDecrypt((message) => {
        if (!available) throw new Error("Temporary key lookup failure");
        return Promise.resolve({ ...message, text: "recovered" });
      }, () => {}),
    },
  });
  owner.on =
    ((event: string, handler: (value: TalkMessage) => Promise<void>) => {
      handlers.set(event, handler);
      return owner;
    }) as never;
  owner.listen = (() => {}) as never;
  const listener = createListener({
    retryClock: timer.clock,
    getClient: () => owner,
    isListening: () => listening,
    setListening: (value) => {
      listening = value;
    },
    summarizeTalkMessage: (message) =>
      Promise.resolve({
        ...message.raw,
        decryptionFailed:
          (message.raw.contentMetadata as Json)[DECRYPTION_FAILED] === "1",
      }),
    observeMessage: (message) => {
      observed.push(String(message.id));
    },
    appendMessage: (_chat, message) => {
      cached.push(String(message.id));
    },
    storeBoxCursor: (_chat, value) => {
      cursors.push(String(value?.messageId));
    },
    upsertChatFromMessage: (message) => {
      upserts.push(String(message.id));
      return Promise.resolve();
    },
    emitEvent: (event, payload) => {
      events.push({ event, payload });
    },
    handleIncomingCall: () => {},
    handleCallCancel: () => {},
    hydrateLiveMedia: () => Promise.resolve(),
    upsertChatFromContact: () => Promise.resolve(),
    handleReadReceipt: () => false,
  });
  listener.start();
  const failed = new TalkMessage({ client: owner, raw: raw() as never });
  await handlers.get("message")!(failed);
  await handlers.get("message")!(failed);
  const latest = { ...raw("2"), contentMetadata: {}, text: "newest" };
  await handlers.get("message")!(
    new TalkMessage({ client: owner, raw: latest as never }),
  );
  await timer.next(); // A failed retry must not emit another message.
  available = true;
  await timer.next();
  assertEquals(events.map((e) => e.event), [
    "listening",
    "message",
    "message",
    "message_recovered",
  ]);
  assertEquals(events.at(-1)?.payload?.accountMid, "u-self");
  assertEquals((events.at(-1)?.payload?.message as Json).text, "recovered");
  assertEquals(cursors, ["1", "2"]);
  assertEquals(observed, ["1", "2"]);
  assertEquals(upserts, ["1", "2"]);
  assertEquals(cached, ["1", "2", "1"]);
  await handlers.get("message")!(failed); // Never regress a repaired row.
  assertEquals(events.length, 4);
  assertEquals(timer.size(), 0);

  // A historical failure can recover even without a live message delivery.
  listener.retryFailedMessage(owner, raw("3", 1));
  await timer.next();
  assertEquals(events.at(-1)?.event, "message_recovered");
  assertEquals((events.at(-1)?.payload?.message as Json).id, "3");
  assertEquals(cursors, ["1", "2"]);

  // A successful live redelivery upgrades the row and cancels its retry timer.
  await handlers.get("message")!(
    new TalkMessage({ client: owner, raw: raw("4") as never }),
  );
  assertEquals(timer.size(), 1);
  await handlers.get("message")!(
    new TalkMessage({
      client: owner,
      raw: { ...raw("4"), contentMetadata: {}, text: "redelivered" } as never,
    }),
  );
  assertEquals(events.at(-1)?.event, "message_recovered");
  assertEquals((events.at(-1)?.payload?.message as Json).text, "redelivered");
  assertEquals(timer.size(), 0);
  assertEquals(cursors, ["1", "2", "4"]);
});
