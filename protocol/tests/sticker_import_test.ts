import { assertEquals } from "@std/assert";
import type { Client } from "@evex/linejs";
import { stickerPackageId } from "../src/sticker_import.ts";
import { createStickerService } from "../src/stickers.ts";
import { policyFor } from "../src/cache_policy.ts";

Deno.test("sticker pack input accepts official links and rejects arbitrary URLs", () => {
  for (
    const input of [
      " 2 ",
      "https://store.line.me/stickershop/product/2/en?from=search",
      "https://line.me/S/sticker/2",
    ]
  ) assertEquals(stickerPackageId(input), "2");
  for (
    const input of [
      "",
      "0",
      "../2",
      "https://example.com/stickershop/product/2/en",
      "https://store.line.me.evil.test/stickershop/product/2/en",
      "https://user@store.line.me/stickershop/product/2/en",
      "http://store.line.me/stickershop/product/2/en",
      "https://store.line.me/stickershop/product/2bad/en",
    ]
  ) assertEquals(stickerPackageId(input), null, input);
});

function fixture(directory: string) {
  const results: Record<string, unknown>[] = [];
  const errors: string[] = [];
  const downloads: string[] = [];
  const images = new Set<string>();
  const policy = policyFor("smart");
  const service = createStickerService({
    getClient: () =>
      ({
        base: {
          request: { request: () => Promise.resolve({ productList: [] }) },
        },
      }) as unknown as Client,
    dataDir: directory,
    stickerDir: directory,
    refreshCachePolicy: () => Promise.resolve(policy),
    cachePolicy: () => policy,
    existingImage: (path) => Promise.resolve(images.has(path) ? path : null),
    cacheUrl: (url, path) => {
      downloads.push(url);
      images.add(path);
      return Promise.resolve(path);
    },
    sentMessagePayload: () => ({}),
    invalidateMessages: () => {},
    invalidateChats: () => {},
    touchChatPreviewFromMessage: () => {},
    ok: (_id, result) => results.push(result as Record<string, unknown>),
    fail: (_id, error) => errors.push(error),
  });
  return { service, results, errors, downloads };
}

Deno.test("imported packs persist without ownership and reuse cached static PNGs", async () => {
  const directory = await Deno.makeTempDir();
  const originalFetch = globalThis.fetch;
  let metadataRequests = 0;
  globalThis.fetch = (() => {
    metadataRequests++;
    return Promise.resolve(Response.json({
      title: "Brown",
      version: "3",
      stickers: [{ id: 10 }, { id: 11 }, { id: "../invalid" }],
    }));
  }) as typeof fetch;
  try {
    const { service, results, errors, downloads } = fixture(directory);
    await service.list(1, "https://store.line.me/stickershop/product/2/en");
    assertEquals(errors, []);
    const packs = results[0].packs as Record<string, unknown>[];
    assertEquals(packs[0].name, "Brown");
    assertEquals(packs[0].imageOnly, true);
    assertEquals((packs[0].stickers as unknown[]).length, 2);
    assertEquals(downloads.length, 2);
    assertEquals(
      downloads.every((url) => url.endsWith("/android/sticker.png")),
      true,
    );
    await service.list(2);
    assertEquals(downloads.length, 2);
    assertEquals(metadataRequests, 1);
    // A fresh service restores the pack manifest without another metadata fetch.
    const restored = fixture(directory);
    await restored.service.list(3);
    const restoredPacks = restored.results[0].packs as Record<
      string,
      unknown
    >[];
    assertEquals(restoredPacks[0].name, "Brown");
    assertEquals(restoredPacks[0].imageOnly, true);
    assertEquals(metadataRequests, 1);
  } finally {
    globalThis.fetch = originalFetch;
    await Deno.remove(directory, { recursive: true });
  }
});

Deno.test("invalid or unavailable pack imports leave the saved library intact", async () => {
  const directory = await Deno.makeTempDir();
  const originalFetch = globalThis.fetch;
  globalThis.fetch =
    (() =>
      Promise.resolve(new Response(null, { status: 404 }))) as typeof fetch;
  try {
    const saved = JSON.stringify([]);
    await Deno.writeTextFile(`${directory}/image-packs.json`, saved);
    const { service, errors, downloads } = fixture(directory);
    await service.list(1, "https://example.com/2");
    await service.list(2, "123");
    assertEquals(errors, [
      "sticker_import_invalid",
      "sticker_import_unavailable",
    ]);
    assertEquals(downloads, []);
    assertEquals(
      await Deno.readTextFile(`${directory}/image-packs.json`),
      saved,
    );
  } finally {
    globalThis.fetch = originalFetch;
    await Deno.remove(directory, { recursive: true });
  }
});
