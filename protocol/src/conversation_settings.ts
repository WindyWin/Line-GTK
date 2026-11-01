import type { Client } from "@evex/linejs";
import { contactKind } from "./contacts.ts";
import { isProfileMid } from "./user_profiles.ts";

type Runtime = {
  getClient: () => Client | null;
  selfMid: () => string;
};
type Person = { mid: string; name: string };

/** Group membership is always read from LINE; do not reuse the chat preview cache. */
export function createConversationSettings(runtime: Runtime) {
  const inviting = new Set<string>();

  function session(chatMid: unknown) {
    if (
      typeof chatMid !== "string" || !/^c[a-zA-Z0-9_-]{1,128}$/.test(chatMid)
    ) {
      throw new Error("invalid_group");
    }
    const client = runtime.getClient();
    const account = runtime.selfMid();
    if (!client || !account) throw new Error("not_logged_in");
    const check = () => {
      if (runtime.getClient() !== client || runtime.selfMid() !== account) {
        throw new Error("session_changed");
      }
    };
    return { client, account, chatMid, check };
  }
  type Session = ReturnType<typeof session>;

  async function group(s: Session) {
    const response = await s.client.base.talk.getChats({
      chatMids: [s.chatMid],
      withMembers: true,
      withInvitees: true,
    });
    s.check();
    const chat = response.chats?.find((c) => c.chatMid === s.chatMid);
    const extra = chat?.extra?.groupExtra;
    if (!chat || !extra || !Object.hasOwn(extra.memberMids ?? {}, s.account)) {
      throw new Error("group_unavailable");
    }
    const members = Object.keys(extra.memberMids).filter(isProfileMid);
    const invitees = Object.keys(extra.inviteeMids ?? {}).filter(isProfileMid)
      .filter((mid) => !members.includes(mid));
    return { chat, members, invitees };
  }

  async function people(s: Session, mids: string[], friendsOnly = false) {
    const found = new Map<string, Person>();
    if (!friendsOnly) {
      for (const mid of mids) found.set(mid, { mid, name: mid });
    }
    for (let i = 0; i < mids.length; i += 100) {
      s.check();
      try {
        const contacts = await s.client.base.talk.getContacts({
          mids: mids.slice(i, i + 100),
        });
        for (const contact of contacts) {
          if (!mids.includes(contact.mid)) continue;
          if (friendsOnly && contactKind(contact) === "bot") continue;
          found.set(contact.mid, {
            mid: contact.mid,
            name: contact.displayNameOverridden?.trim() ||
              contact.displayName?.trim() || contact.mid,
          });
        }
      } catch (error) {
        if (friendsOnly) throw error;
        // A name lookup failure must not hide members or pending invitations.
      }
      s.check();
    }
    return [...found.values()].sort((a, b) => a.name.localeCompare(b.name));
  }

  async function details(chatMid: unknown) {
    const s = session(chatMid);
    const { chat, members, invitees } = await group(s);
    const names = await people(s, [...members, ...invitees]);
    s.check();
    return {
      chatMid: s.chatMid,
      name: chat.chatName || s.chatMid,
      members: names.filter((p) => members.includes(p.mid)),
      invitees: names.filter((p) => invitees.includes(p.mid)),
    };
  }

  async function candidates(chatMid: unknown) {
    const s = session(chatMid);
    const { members, invitees } = await group(s);
    const friends = await s.client.base.talk.getAllContactIds({});
    s.check();
    const excluded = new Set([s.account, ...members, ...invitees]);
    const mids = [...new Set(friends)].filter(isProfileMid)
      .filter((mid) => !excluded.has(mid));
    const contacts = await people(s, mids, true);
    s.check();
    return { chatMid: s.chatMid, candidates: contacts };
  }

  async function invite(chatMid: unknown, targets: unknown) {
    const s = session(chatMid);
    if (
      !Array.isArray(targets) || targets.length === 0 || targets.length > 100 ||
      !targets.every(isProfileMid) || targets.includes(s.account)
    ) throw new Error("invalid_invitees");
    const key = `${s.account}:${s.chatMid}`;
    if (inviting.has(key)) throw new Error("invitation_in_progress");
    inviting.add(key);
    try {
      const allowed = await candidates(s.chatMid);
      s.check();
      const mids = [...new Set(targets)];
      if (!mids.every((mid) => allowed.candidates.some((p) => p.mid === mid))) {
        throw new Error("invitees_changed");
      }
      await s.client.base.talk.inviteIntoChat({
        chatMid: s.chatMid,
        targetUserMids: mids,
      });
      s.check();
      return { chatMid: s.chatMid, invited: mids };
    } finally {
      inviting.delete(key);
    }
  }
  return { details, candidates, invite };
}
