import { type Client, TalkMessage } from "@evex/linejs";
import type * as calls from "./calls.ts";
import { conversationMid } from "./messages.ts";
import type { ReadOperation } from "./read_receipts.ts";
import { createReceiverWatchdog } from "./receiver_watchdog.ts";
import { createDecryptionRetry, type RetryClock } from "./decryption_retry.ts";

type Json = Record<string, unknown>;

export type ListenerRuntime = {
  retryClock?: RetryClock;
  prepareMessage?: (message: Json) => Promise<Json | null>;
  handleIncomingCall: typeof calls.handleIncomingCall;
  handleCallCancel: typeof calls.handleCallCancel;
  shouldIgnoreMessage?: (message: Json) => boolean;
  getClient: () => Client | null;
  observeMessage?: (message: Json) => void;
  refreshMessageSender?: (mid: string) => void;
  isListening: () => boolean;
  setListening: (listening: boolean) => void;
  receiverStalled?: () => void;
  appendMessage: (chatMid: string, message: Json) => void;
  summarizeTalkMessage: (
    message: TalkMessage,
    opts?: { withMedia?: boolean },
  ) => Promise<Json>;
  storeBoxCursor: (
    mid: string,
    value?: { messageId?: unknown; deliveredTime?: unknown } | null,
  ) => void;
  upsertChatFromMessage: (message: Json) => Promise<void>;
  hydrateLiveMedia: (
    message: TalkMessage,
    summary: Json,
    chatMid: string,
  ) => Promise<void>;
  upsertChatFromContact: (mid: string) => Promise<void>;
  emitEvent: (event: string, payload?: Json) => void;
  handleReadReceipt: (operation: ReadOperation) => boolean;
};

export function createListener(runtime: ListenerRuntime) {
  // Failed deliveries can be upgraded once, without replaying unread/cursor work.
  const delivered = new Map<string, boolean>();
  let owner: Client | null = null;
  let stopWatchdog: (() => void) | undefined;
  const {
    summarizeTalkMessage,
    storeBoxCursor,
    upsertChatFromMessage,
    hydrateLiveMedia,
    upsertChatFromContact,
    emitEvent,
  } = runtime;
  const retries = createDecryptionRetry({
    getClient: runtime.getClient,
    clock: runtime.retryClock,
    decrypt: async (client, raw) =>
      await client.base.e2ee.decryptE2EEMessage(
        raw as never,
      ) as unknown as Json,
    recovered: async (client, raw) => {
      await deliver(
        client,
        new TalkMessage({ client, raw: raw as never }),
        true,
      );
    },
  });

  async function deliver(client: Client, tm: TalkMessage, recovered = false) {
    try {
      if (runtime.getClient() !== client) return;
      const account = client.base.profile?.mid;
      let message = await summarizeTalkMessage(tm, { withMedia: false });
      if (runtime.prepareMessage) {
        const prepared = await runtime.prepareMessage(message);
        if (!prepared) return;
        message = prepared;
      }
      // 交換直前に受信済みの本文は同じアカウントに限って届け、取りこぼさない。
      const active = runtime.getClient();
      if (recovered && active !== client) return;
      if (
        !active ||
        (active !== client &&
          (!account || active.base.profile?.mid !== account))
      ) return;
      if (runtime.shouldIgnoreMessage?.(message)) return;
      const peer = conversationMid(message);
      const successful = message.decryptionFailed !== true;
      if (message.id) {
        const key = `${account}:${peer}:${message.id}`;
        const previous = delivered.get(key);
        if (previous === true || (previous === false && !successful)) return;
        recovered ||= previous === false;
        delivered.set(key, successful);
        if (delivered.size > 2048) {
          delivered.delete(delivered.keys().next().value!);
        }
      }
      const raw = (tm.raw ?? message) as unknown as Json;
      if (successful) retries.forget(client, raw);
      else retries.enqueue(client, raw);
      if (!recovered) runtime.observeMessage?.(message);
      runtime.appendMessage(peer, message);
      emitEvent(recovered ? "message_recovered" : "message", {
        accountMid: account,
        message: { ...message, imagePath: null },
      });
      if (!recovered) {
        // Recovery of an older row must not move the newest-message cursor back.
        storeBoxCursor(peer, {
          messageId: message.id,
          deliveredTime: message.createdTime,
        });
        await upsertChatFromMessage(message);
      }
      if (message.needsMedia) {
        // Use the live TalkMessage.getData path — history refetch often misses
        // brand-new messages and left bubbles on "Image unavailable" until restart.
        const mediaClient = runtime.getClient();
        if (
          mediaClient && account && mediaClient.base.profile?.mid === account
        ) {
          const mediaMessage = mediaClient === client
            ? tm
            : new TalkMessage({ client: mediaClient, raw: tm.raw });
          void hydrateLiveMedia(mediaMessage, message, peer);
        }
      }
    } catch (e) {
      console.error("[listen message]", e);
    }
  }

  function startListen() {
    const client = runtime.getClient();
    if (!client || runtime.isListening()) return;
    if (owner !== client) {
      retries.reset();
      owner = client;
    }
    runtime.setListening(true);

    // Register BEFORE listen() so early ops are not missed.
    client.on("message", (msg) => deliver(client, msg as TalkMessage));

    client.on("event", (op: {
      type?: string | number;
      param1?: string;
      param2?: string;
      param3?: string;
    }) => {
      try {
        if (runtime.getClient() !== client) return;
        const type = String(op?.type ?? "");
        if (runtime.handleReadReceipt(op)) {
          return;
        } else if (
          type.includes("ADD_CONTACT") ||
          type.includes("ACCEPT_CONTACT") ||
          type.includes("FRIEND_REQUEST") ||
          type === "NOTIFIED_UPDATE_PROFILE"
        ) {
          const mid = String(op.param1 ?? op.param2 ?? "");
          if (type === "NOTIFIED_UPDATE_PROFILE" && mid) {
            runtime.refreshMessageSender?.(mid);
          }
          if (mid) void upsertChatFromContact(mid);
        }
      } catch (e) {
        console.error("[listen event]", e);
      }
    });

    client.on(
      "call:incoming",
      (event) => {
        if (runtime.getClient() === client) runtime.handleIncomingCall(event);
      },
    );
    client.on(
      "call:cancel",
      (event) => {
        if (runtime.getClient() === client) runtime.handleCallCancel(event);
      },
    );

    client.listen({ talk: true, square: false });
    if (runtime.receiverStalled) {
      stopWatchdog?.();
      const watchdog = createReceiverWatchdog(client);
      const timer = setInterval(() => {
        if (runtime.getClient() !== client || !client.base.authToken) {
          stop();
        } else if (watchdog.stalled()) {
          stop();
          runtime.setListening(false);
          runtime.receiverStalled?.();
        }
      }, 5_000);
      const stop = () => {
        clearInterval(timer);
        watchdog.stop();
      };
      stopWatchdog = stop;
    }
    emitEvent("listening");
  }

  return {
    start: startListen,
    cancelRetries: retries.reset,
    retryFailedMessage: retries.enqueue,
  };
}
