import type { Client } from "@evex/linejs";
import { chatEventOf } from "./chat_events.ts";

type Json = Record<string, unknown>;
type Runtime = {
  getClient: () => Client | null;
  selfMid: () => string;
  knownName: (mid: string) => string | undefined;
  emit: (mid: string, name: string) => void;
  knownPicturePath?: (mid: string) => string | null | undefined;
  avatarPathFor?: (
    mid: string,
    picturePath?: string | null,
  ) => Promise<string | null>;
  emitAvatar?: (mid: string, path: string) => void;
  now?: () => number;
};
type Profile = {
  name: string | null;
  until: number;
  picturePath?: string | null;
};

function userMid(value: unknown): value is string {
  return typeof value === "string" && /^u[^\s]{1,128}$/.test(value);
}

function nameOf(value: unknown, mid: string): string | null {
  if (typeof value !== "string") return null;
  const name = value.trim();
  return name && name !== mid ? name : null;
}

export function createMessageSenders(runtime: Runtime) {
  const now = runtime.now ?? Date.now;
  let owner: Client | null = null;
  let ownerMid = "";
  let epoch = 0;
  let active = 0;
  let scheduled = false;
  const profiles = new Map<string, Profile>();
  const queue = new Map<string, boolean>();
  const pending = new Map<string, number>();
  const wantedAvatars = new Set<string>();
  const avatarQueue = new Map<string, string | null | undefined>();
  const avatarPending = new Map<string, number>();
  const avatars = new Map<string, { path: string | null; until: number }>();
  let activeAvatars = 0;

  function sync() {
    const client = runtime.getClient();
    const mid = runtime.selfMid();
    if (client !== owner || mid !== ownerMid) {
      owner = client;
      ownerMid = mid;
      epoch++;
      profiles.clear();
      queue.clear();
      pending.clear();
      wantedAvatars.clear();
      avatarQueue.clear();
      avatarPending.clear();
      avatars.clear();
    }
    return client && mid ? client : null;
  }

  function publish(
    mid: string,
    name: string | null,
    picturePath?: string | null,
  ) {
    profiles.delete(mid);
    profiles.set(mid, {
      name,
      until: now() + (name ? 600_000 : 60_000),
      picturePath,
    });
    while (profiles.size > 5000) profiles.delete(profiles.keys().next().value!);
    if (name) runtime.emit(mid, name);
    if (wantedAvatars.has(mid)) queueAvatar(mid, picturePath);
  }

  function queueAvatar(mid: string, picturePath?: string | null) {
    if (!runtime.avatarPathFor || picturePath === undefined) return;
    const cached = avatars.get(mid);
    if (cached && cached.until > now()) {
      if (cached.path) runtime.emitAvatar?.(mid, cached.path);
      return;
    }
    if (!avatarPending.has(mid)) avatarQueue.set(mid, picturePath);
    pumpAvatars();
  }

  function pumpAvatars() {
    const client = sync();
    if (!client || !runtime.avatarPathFor) return;
    while (activeAvatars < 2 && avatarQueue.size) {
      const [mid, picturePath] = avatarQueue.entries().next().value!;
      avatarQueue.delete(mid);
      const generation = epoch;
      const sessionMid = ownerMid;
      avatarPending.set(mid, generation);
      activeAvatars++;
      void (async () => {
        try {
          const path = await runtime.avatarPathFor!(mid, picturePath);
          if (
            epoch !== generation || runtime.getClient() !== client ||
            runtime.selfMid() !== sessionMid
          ) return;
          avatars.set(mid, { path, until: now() + (path ? 600_000 : 60_000) });
          while (avatars.size > 5000) {
            avatars.delete(avatars.keys().next().value!);
          }
          if (path) runtime.emitAvatar?.(mid, path);
        } catch {
          // Missing pictures leave the reader's initials visible and remain retryable.
        } finally {
          if (avatarPending.get(mid) === generation) avatarPending.delete(mid);
          activeAvatars--;
          pumpAvatars();
        }
      })();
    }
  }

  function schedule() {
    if (scheduled) return;
    scheduled = true;
    queueMicrotask(() => {
      scheduled = false;
      pump();
    });
  }

  function pump() {
    const client = sync();
    if (!client) return;
    while (active < 2 && queue.size) {
      const entries = [...queue];
      const single = entries[0][1];
      const mids = entries.filter(([, one]) => one === single)
        .slice(0, single ? 1 : 20).map(([mid]) => mid);
      const generation = epoch;
      const sessionMid = ownerMid;
      for (const mid of mids) {
        queue.delete(mid);
        pending.set(mid, generation);
      }
      active++;
      const current = () =>
        epoch === generation && runtime.getClient() === client &&
        runtime.selfMid() === sessionMid;
      void (async () => {
        try {
          const contacts = single
            ? [await client.base.talk.getContact({ mid: mids[0] })]
            : await client.base.talk.getContacts({ mids });
          if (!current()) return;
          const found = new Map(
            contacts.map((contact) => [contact.mid, contact]),
          );
          for (const mid of mids) {
            const contact = found.get(mid);
            if (!contact && !single) {
              queue.set(mid, true);
              continue;
            }
            publish(
              mid,
              nameOf(contact?.displayNameOverridden, mid) ||
                nameOf(contact?.displayName, mid),
              contact?.picturePath ?? null,
            );
          }
        } catch {
          if (!current()) return;
          for (const mid of mids) {
            // 1人の取得失敗で同じグループのほかの送信者まで未表示にしない。
            if (!single) queue.set(mid, true);
            else publish(mid, null);
          }
        } finally {
          for (const mid of mids) {
            if (pending.get(mid) === generation) pending.delete(mid);
          }
          active--;
          schedule();
        }
      })();
    }
  }

  function observe(messages: Json[]) {
    if (!sync()) return;
    const mids = new Set<string>();
    for (const message of messages) {
      if (typeof message.to !== "string" || !/^[cr]/.test(message.to)) continue;
      if (!message.mine && userMid(message.from)) mids.add(message.from);
      const event = chatEventOf(message);
      if (event) {
        for (const mid of [event.actorMid, ...event.targetMids]) {
          if (userMid(mid)) mids.add(mid);
        }
      }
      const reply = message.replyTo as Json | null | undefined;
      if (reply && !reply.mine && userMid(reply.from)) mids.add(reply.from);
    }
    resolve([...mids]);
  }

  function resolve(mids: string[], withAvatars = false) {
    if (!sync()) return;
    for (const mid of new Set(mids)) {
      if (!userMid(mid)) continue;
      if (mid === ownerMid) continue;
      if (withAvatars) wantedAvatars.add(mid);
      const cached = profiles.get(mid);
      if (cached && cached.until > now()) {
        if (cached.name) runtime.emit(mid, cached.name);
        if (!withAvatars) continue;
        if (cached.picturePath !== undefined) {
          queueAvatar(mid, cached.picturePath);
          continue;
        }
      }
      const known = !cached ? nameOf(runtime.knownName(mid), mid) : null;
      if (known) {
        const picturePath = runtime.knownPicturePath?.(mid);
        publish(mid, known, picturePath);
        if (!withAvatars || picturePath !== undefined) continue;
      }
      if (!pending.has(mid)) queue.set(mid, false);
    }
    // 本文の返却・新着通知は名前の通信完了を待たない。
    schedule();
  }

  function refresh(mid: string) {
    if (!sync() || !userMid(mid) || !profiles.has(mid)) return;
    profiles.delete(mid);
    if (!pending.has(mid)) queue.set(mid, false);
    schedule();
  }

  return { observe, resolve, refresh };
}
