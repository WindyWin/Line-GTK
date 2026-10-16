import type { Client } from "@evex/linejs";
import { join } from "@std/path";
import { atomicWriteJson } from "./storage.ts";

type Json = Record<string, unknown>;
export type ChatOperation = {
  type?: string | number;
  revision?: unknown;
  createdTime?: unknown;
  param1?: string;
  param2?: string;
  param3?: string;
};
export type ChatEvent = {
  kind: "invited" | "joined" | "left" | "removed";
  actorMid: string;
  targetMids: string[];
};
type EventMessage = {
  id: string;
  to: string;
  from: string;
  mine: false;
  text: string;
  createdTime: number;
  contentType: "CHAT_EVENT";
  chatEvent: ChatEvent;
};
const operations: Record<string, [ChatEvent["kind"], boolean]> = {};
for (
  const [kind, self, names] of [
    ["invited", false, [
      "13",
      "22",
      "124",
      "NOTIFIED_INVITE_INTO_GROUP",
      "NOTIFIED_INVITE_INTO_ROOM",
      "NOTIFIED_INVITE_INTO_CHAT",
    ]],
    ["invited", true, [
      "12",
      "21",
      "123",
      "INVITE_INTO_GROUP",
      "INVITE_INTO_ROOM",
      "INVITE_INTO_CHAT",
    ]],
    ["joined", false, [
      "17",
      "60",
      "130",
      "NOTIFIED_ACCEPT_GROUP_INVITATION",
      "NOTIFIED_JOIN_CHAT",
      "NOTIFIED_ACCEPT_CHAT_INVITATION",
    ]],
    ["joined", true, [
      "16",
      "129",
      "ACCEPT_GROUP_INVITATION",
      "ACCEPT_CHAT_INVITATION",
    ]],
    ["left", false, [
      "15",
      "24",
      "61",
      "128",
      "NOTIFIED_LEAVE_GROUP",
      "NOTIFIED_LEAVE_ROOM",
      "NOTIFIED_LEAVE_CHAT",
      "NOTIFIED_DELETE_SELF_FROM_CHAT",
    ]],
    ["left", true, [
      "14",
      "23",
      "127",
      "LEAVE_GROUP",
      "LEAVE_ROOM",
      "DELETE_SELF_FROM_CHAT",
    ]],
    ["removed", false, [
      "19",
      "133",
      "NOTIFIED_KICKOUT_FROM_GROUP",
      "NOTIFIED_DELETE_OTHER_FROM_CHAT",
    ]],
    ["removed", true, [
      "18",
      "132",
      "KICKOUT_FROM_GROUP",
      "DELETE_OTHER_FROM_CHAT",
    ]],
  ] as const
) for (const name of names) operations[name] = [kind, self];

const user = (mid: string) => /^u[a-zA-Z0-9_-]{1,128}$/.test(mid);
export function chatEventMessage(
  op: ChatOperation,
  selfMid: string,
  now = Date.now(),
): EventMessage | null {
  const definition = operations[String(op.type)];
  const chatMid = op.param1 ?? "";
  if (
    !definition || !user(selfMid) || !/^[cr][a-zA-Z0-9_-]{1,128}$/.test(chatMid)
  ) return null;
  const [kind, own] = definition;
  const actorMid = own ? selfMid : op.param2 ?? "";
  if (!user(actorMid)) return null;
  const rawTargets = own ? op.param2 : op.param3;
  const targetMids = kind === "invited" || kind === "removed"
    ? [...new Set((rawTargets ?? "").split("\x1e").filter(user))].slice(0, 100)
    : [];
  if ((kind === "invited" || kind === "removed") && !targetMids.length) {
    return null;
  }
  const revision = String(op.revision ?? "");
  const time = Number(op.createdTime);
  const createdTime = Number.isFinite(time) && time > 0
    ? (time < 1e12 ? time * 1000 : time)
    : now;
  // 履歴のメッセージIDと混ぜず、再送された同じ操作は同じIDにする。
  const identity = /^\d+$/.test(revision) && revision !== "0"
    ? revision
    : `${createdTime}:${kind}:${actorMid}:${targetMids.join(",")}`;
  return {
    id: `event:${identity}`,
    to: chatMid,
    from: actorMid,
    mine: false,
    text: "",
    createdTime,
    contentType: "CHAT_EVENT",
    chatEvent: { kind, actorMid, targetMids },
  };
}

type Runtime = {
  directory: string;
  getClient: () => Client | null;
  selfMid: () => string;
  observeProfiles: (messages: Json[]) => void;
  emit: (event: string, payload: Json) => void;
};
export function createChatEvents(runtime: Runtime) {
  let owner = "";
  let records: EventMessage[] | null = null;
  let queue = Promise.resolve();
  function reset() {
    owner = "";
    records = null;
  }
  async function load(account: string): Promise<EventMessage[]> {
    if (owner !== account) {
      owner = account;
      records = null;
    }
    if (records) return records;
    let loaded: EventMessage[] = [];
    try {
      const raw = JSON.parse(
        await Deno.readTextFile(
          join(runtime.directory, `events-${account}.json`),
        ),
      );
      if (Array.isArray(raw)) {
        loaded = raw.filter((m) =>
          m?.contentType === "CHAT_EVENT" && typeof m.id === "string" &&
          m.id.startsWith("event:") &&
          typeof m.to === "string" && /^[cr][a-zA-Z0-9_-]{1,128}$/.test(m.to) &&
          Number.isFinite(m.createdTime) && m.createdTime > 0 &&
          ["invited", "joined", "left", "removed"].includes(
            m.chatEvent?.kind,
          ) &&
          user(m.chatEvent?.actorMid) &&
          Array.isArray(m.chatEvent?.targetMids) &&
          m.chatEvent.targetMids.length <= 100 &&
          m.chatEvent.targetMids.every(user)
        ).slice(-5000);
      }
    } catch { /* 初回またはキャッシュが消去された場合は空の履歴から始める。 */ }
    if (runtime.selfMid() !== account) return [];
    records = loaded;
    return loaded;
  }
  function observe(message: EventMessage) {
    runtime.observeProfiles(
      [message.chatEvent.actorMid, ...message.chatEvent.targetMids].map(
        (from) => ({ from, to: message.to, mine: false }),
      ),
    );
  }
  function handleOperation(op: ChatOperation): boolean {
    const account = runtime.selfMid();
    const client = runtime.getClient();
    const message = chatEventMessage(op, account);
    if (!message || !client) return false;
    const active = () =>
      runtime.selfMid() === account && runtime.getClient() === client;
    queue = queue.then(async () => {
      if (!active()) return;
      const loaded = await load(account);
      if (
        !active() ||
        loaded.some((m) => m.id === message.id && m.to === message.to)
      ) return;
      const sameChat = loaded.filter((m) => m.to === message.to).slice(-499);
      records = [
        ...loaded.filter((m) => m.to !== message.to),
        ...sameChat,
        message,
      ].sort((a, b) => a.createdTime - b.createdTime).slice(-5000);
      observe(message);
      runtime.emit("chat_event", { accountMid: account, message });
      await atomicWriteJson(
        join(runtime.directory, `events-${account}.json`),
        records,
      );
    }).catch(() =>
      console.error("[chat-events] グループイベントを保存できませんでした。")
    );
    return true;
  }
  async function merge(chatMid: string, messages: Json[]): Promise<Json[]> {
    const account = runtime.selfMid();
    const client = runtime.getClient();
    if (!user(account) || !/^[cr]/.test(chatMid)) return messages;
    await queue;
    const loaded = await load(account);
    if (runtime.selfMid() !== account || runtime.getClient() !== client) {
      return messages;
    }
    const oldest = messages.length
      ? Math.min(...messages.map((m) => Number(m.createdTime ?? 0)))
      : 0;
    const events = loaded.filter((m) =>
      m.to === chatMid && m.createdTime >= oldest
    ).slice(-500);
    events.forEach(observe);
    const combined = new Map(messages.map((m) => [String(m.id), m]));
    for (const event of events) combined.set(event.id, event);
    return [...combined.values()].sort((a, b) =>
      Number(a.createdTime) - Number(b.createdTime)
    );
  }
  return { handleOperation, merge, reset, settled: () => queue };
}
