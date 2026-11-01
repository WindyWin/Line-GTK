import { assertEquals, assertRejects } from "@std/assert";
import type { Client } from "@evex/linejs";
import { createConversationSettings } from "../src/conversation_settings.ts";

function fixture() {
  let account = "u-self";
  let members = ["u-self", "u-member"];
  const invitees = ["u-invited"];
  let lookup: () => Promise<void> = () => Promise.resolve();
  let mutation: () => Promise<void> = () => Promise.resolve();
  const sent: { chatMid: string; targetUserMids: string[] }[] = [];
  const reads: unknown[] = [];
  const client = {
    base: {
      talk: {
        getChats: (options: unknown) => {
          reads.push(options);
          return Promise.resolve({
            chats: [{
              chatMid: "c-group",
              chatName: "Friends <&> 家族",
              extra: {
                groupExtra: {
                  memberMids: Object.fromEntries(members.map((id) => [id, 1])),
                  inviteeMids: Object.fromEntries(
                    invitees.map((id) => [id, 2]),
                  ),
                },
              },
            }],
          });
        },
        getAllContactIds: () =>
          Promise.resolve([
            "u-self",
            "u-member",
            "u-invited",
            "u-friend",
            "u-friend",
            "u-bot",
            "not-a-user",
          ]),
        getContacts: async ({ mids }: { mids: string[] }) => {
          await lookup();
          return mids.map((mid) => ({
            mid,
            displayName: `Name ${mid}`,
            displayNameOverridden: mid === "u-friend" ? "My friend" : "",
            type: mid === "u-bot" ? "BOT" : "MID",
          }));
        },
        inviteIntoChat: async (
          args: { chatMid: string; targetUserMids: string[] },
        ) => {
          sent.push(args);
          await mutation();
          invitees.push(...args.targetUserMids);
        },
      },
    },
  } as unknown as Client;
  let current: Client | null = client;
  const service = createConversationSettings({
    getClient: () => current,
    selfMid: () => account,
  });
  return {
    service,
    sent,
    reads,
    logout: () => {
      current = null;
    },
    switchAccount: () => {
      account = "u-other";
    },
    replaceClient: () => {
      current = { ...client } as Client;
    },
    leave: () => {
      members = ["u-member"];
    },
    joined: () => {
      members.push("u-friend");
    },
    names: (fn: typeof lookup) => {
      lookup = fn;
    },
    mutate: (fn: typeof mutation) => {
      mutation = fn;
    },
  };
}

Deno.test("group settings separate members and invitees and request both from LINE", async () => {
  const f = fixture();
  const details = await f.service.details("c-group");
  assertEquals(details.name, "Friends <&> 家族");
  assertEquals(details.members.map((p) => p.mid).sort(), [
    "u-member",
    "u-self",
  ]);
  assertEquals(details.invitees.map((p) => p.mid), ["u-invited"]);
  assertEquals(f.reads, [{
    chatMids: ["c-group"],
    withMembers: true,
    withInvitees: true,
  }]);
});

Deno.test("invite picker excludes self, members, invitees, bots and duplicate friends", async () => {
  const f = fixture();
  assertEquals(await f.service.candidates("c-group"), {
    chatMid: "c-group",
    candidates: [{ mid: "u-friend", name: "My friend" }],
  });
  f.joined();
  assertEquals((await f.service.candidates("c-group")).candidates, []);
});

Deno.test("invitation submits deduplicated selected friends and refresh shows pending state", async () => {
  const f = fixture();
  assertEquals(await f.service.invite("c-group", ["u-friend", "u-friend"]), {
    chatMid: "c-group",
    invited: ["u-friend"],
  });
  assertEquals(f.sent, [{ chatMid: "c-group", targetUserMids: ["u-friend"] }]);
  assertEquals((await f.service.candidates("c-group")).candidates, []);
  assertEquals((await f.service.details("c-group")).invitees.length, 2);
});

Deno.test("invalid targets and membership changes cannot send invitations", async () => {
  const f = fixture();
  for (
    const targets of [
      [],
      ["u-self"],
      ["bad"],
      "u-friend",
      Array(101).fill("u-friend"),
    ]
  ) {
    await assertRejects(
      () => f.service.invite("c-group", targets),
      Error,
      "invalid_invitees",
    );
  }
  for (
    const targets of [["u-member"], ["u-invited"], ["u-bot"], ["u-stranger"]]
  ) {
    await assertRejects(
      () => f.service.invite("c-group", targets),
      Error,
      "invitees_changed",
    );
  }
  f.joined();
  await assertRejects(
    () => f.service.invite("c-group", ["u-friend"]),
    Error,
    "invitees_changed",
  );
  assertEquals(f.sent, []);
});

Deno.test("group settings reject invalid chats and non-member access", async () => {
  const f = fixture();
  for (const mid of ["u-friend", "r-room", "c../bad", null]) {
    await assertRejects(() => f.service.details(mid), Error, "invalid_group");
  }
  await assertRejects(
    () => f.service.details("c-missing"),
    Error,
    "group_unavailable",
  );
  f.leave();
  await assertRejects(
    () => f.service.details("c-group"),
    Error,
    "group_unavailable",
  );
  await assertRejects(
    () => f.service.invite("c-group", ["u-friend"]),
    Error,
    "group_unavailable",
  );
  assertEquals(f.sent, []);
});

Deno.test("name failures preserve roster identities but cannot create invite candidates", async () => {
  const f = fixture();
  f.names(() => Promise.reject(new Error("offline")));
  assertEquals((await f.service.details("c-group")).members.length, 2);
  await assertRejects(() => f.service.candidates("c-group"), Error, "offline");
  await assertRejects(
    () => f.service.invite("c-group", ["u-friend"]),
    Error,
    "offline",
  );
  assertEquals(f.sent, []);
});

Deno.test("logout, account switch and reconnect discard stale roster and invite requests", async () => {
  for (const change of ["logout", "switchAccount", "replaceClient"] as const) {
    for (const operation of ["details", "invite"] as const) {
      const f = fixture();
      f.names(() => {
        f[change]();
        return Promise.resolve();
      });
      await assertRejects(
        () =>
          operation === "details"
            ? f.service.details("c-group")
            : f.service.invite("c-group", ["u-friend"]),
        Error,
        "session_changed",
      );
      assertEquals(f.sent, []);
    }
  }
});

Deno.test("failed invitations release lock and simultaneous submission sends only once", async () => {
  const f = fixture();
  f.mutate(() => Promise.reject(new Error("server rejected")));
  await assertRejects(
    () => f.service.invite("c-group", ["u-friend"]),
    Error,
    "server rejected",
  );
  let finish!: () => void;
  f.mutate(() =>
    new Promise<void>((resolve) => {
      finish = resolve;
    })
  );
  const first = f.service.invite("c-group", ["u-friend"]);
  await assertRejects(
    () => f.service.invite("c-group", ["u-friend"]),
    Error,
    "invitation_in_progress",
  );
  // Advance until the first request reaches the mocked network mutation.
  while (!finish) await new Promise((resolve) => setTimeout(resolve, 0));
  finish();
  await first;
  assertEquals(f.sent.length, 2);
});
