import { assertEquals } from "@std/assert";
import type { Client } from "@evex/linejs";
import { createOutgoingMedia } from "../src/outgoing_media.ts";

Deno.test("sending a local transparent PNG uploads its original bytes as IMAGE", async () => {
  const directory = await Deno.makeTempDir();
  // A 1x1 RGBA PNG, including its alpha channel.
  const png = Uint8Array.from(
    atob(
      "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAAC0lEQVR42mNgAAIAAAUAAarVyFEAAAAASUVORK5CYII=",
    ),
    (c) => c.charCodeAt(0),
  );
  const source = `${directory}/sticker.png`;
  const uploads: { type: string; data: Blob; name: string }[] = [];
  const results: Record<string, unknown>[] = [];
  const errors: string[] = [];
  const client = {
    base: {
      obs: {
        uploadObjTalk: (
          _to: string,
          type: string,
          data: Blob,
          _unused: unknown,
          name: string,
        ) => {
          uploads.push({ type, data, name });
          return Promise.resolve({ objId: "sent-image" });
        },
      },
      talk: { getPreviousMessagesV2WithRequest: () => Promise.resolve([]) },
    },
  } as unknown as Client;
  const outgoing = createOutgoingMedia({
    getClient: () => client,
    mediaDir: directory,
    mediaDest: (id, ext = "jpg") => `${directory}/${id}.${ext}`,
    fullDest: (id, ext = "jpg") => `${directory}/${id}.full.${ext}`,
    thumbDest: (id) => `${directory}/${id}.thumb.jpg`,
    existingFile: () => Promise.resolve(null),
    existingImage: () => Promise.resolve(null),
    isImageBytes: () => true,
    writeImageFile: async (path, bytes) => {
      await Deno.writeFile(path, bytes);
      return path;
    },
    writeFullImage: () => Promise.resolve(null),
    refetchMessageData: () => Promise.resolve(null),
    uiMediaPath: (_id, path) => Promise.resolve(path),
    isPreviewOrThumbImage: () => Promise.resolve(false),
    getBoxCursor: () => undefined,
    isMineFrom: () => true,
    normType: String,
    getCachedMessages: () => undefined,
    invalidateMessages() {},
    invalidateChats() {},
    sentMessagePayload: () => ({}),
    touchChatPreviewFromMessage() {},
    emitEvent() {},
    ok: (_id, result) => results.push(result as Record<string, unknown>),
    fail: (_id, error) => errors.push(error),
  });
  try {
    await Deno.writeFile(source, png);
    await outgoing.sendMedia(1, "r-test-room", source, "image");
    assertEquals(errors, []);
    assertEquals(uploads.length, 1);
    assertEquals(uploads[0].type, "image");
    assertEquals(uploads[0].data.type, "image/png");
    assertEquals(new Uint8Array(await uploads[0].data.arrayBuffer()), png);
    const message = results[0].message as Record<string, unknown>;
    assertEquals(message.contentType, "IMAGE");
    assertEquals(await Deno.readFile(String(message.imagePath)), png);
  } finally {
    await Deno.remove(directory, { recursive: true });
  }
});
