import type { Client } from "@evex/linejs";
import { DECRYPTION_FAILED } from "./e2ee_guard.ts";

type Json = Record<string, unknown>;
export type RetryClock = {
  now: () => number;
  schedule: (run: () => Promise<void>, delay: number) => () => void;
};
const clock: RetryClock = {
  now: () => performance.now(),
  schedule: (run, delay) => {
    const timer = setTimeout(() => void run(), delay);
    return () => clearTimeout(timer);
  },
};
const WINDOW_MS = 180_000;
const DELAYS = [1_000, 3_000, 10_000, 30_000];
const MAX_PENDING = 128;

type Job = {
  client: Client;
  account: string;
  raw: Json;
  deadline: number;
  attempts: number;
  cancel?: () => void;
};

/** Retain the encrypted envelope; every attempt starts without a failure marker. */
function envelope(raw: Json): Json {
  const contentMetadata = { ...(raw.contentMetadata as Json ?? {}) };
  delete contentMetadata[DECRYPTION_FAILED];
  return { ...raw, contentMetadata };
}

/** Retry failed group messages outside the sequential LINE receive loop. */
export function createDecryptionRetry(runtime: {
  getClient: () => Client | null;
  decrypt: (client: Client, raw: Json) => Promise<Json>;
  recovered: (client: Client, raw: Json) => Promise<void>;
  clock?: RetryClock;
}) {
  const timer = runtime.clock ?? clock;
  const jobs = new Map<string, Job>();
  const key = (client: Client, raw: Json) =>
    `${client.base.profile?.mid}:${raw.to}:${raw.id}`;
  const active = (job: Job) =>
    runtime.getClient() === job.client &&
    job.client.base.profile?.mid === job.account;

  function forget(client: Client, raw: Json) {
    const id = key(client, raw);
    jobs.get(id)?.cancel?.();
    jobs.delete(id);
  }
  function reset() {
    for (const job of jobs.values()) job.cancel?.();
    jobs.clear();
  }
  function schedule(id: string, job: Job) {
    const remaining = job.deadline - timer.now();
    if (!active(job) || remaining <= 0) {
      jobs.delete(id);
      return;
    }
    const delay = Math.min(
      DELAYS[Math.min(job.attempts, DELAYS.length - 1)],
      remaining,
    );
    job.cancel = timer.schedule(async () => {
      if (jobs.get(id) !== job) return;
      if (!active(job) || timer.now() >= job.deadline) {
        jobs.delete(id);
        return;
      }
      job.attempts++;
      try {
        const raw = await runtime.decrypt(job.client, envelope(job.raw));
        if (jobs.get(id) !== job) return;
        if (!active(job) || timer.now() >= job.deadline) {
          jobs.delete(id);
          return;
        }
        const meta = raw.contentMetadata as Json | undefined;
        if (meta?.[DECRYPTION_FAILED] !== "1") {
          await runtime.recovered(job.client, raw);
          if (jobs.get(id) === job) jobs.delete(id);
          return;
        }
      } catch { /* Keep the failed row; never log message or key material. */ }
      if (jobs.get(id) === job) schedule(id, job);
    }, delay);
  }
  function enqueue(client: Client, raw: Json) {
    if (
      !client.base.profile?.mid || !raw.id || !/^[cr]/.test(String(raw.to)) ||
      !Array.isArray(raw.chunks) || raw.chunks.length < 5
    ) return;
    const id = key(client, raw);
    if (jobs.has(id) || jobs.size >= MAX_PENDING) return;
    const job: Job = {
      client,
      account: client.base.profile.mid,
      raw: envelope(raw),
      deadline: timer.now() + WINDOW_MS,
      attempts: 0,
    };
    jobs.set(id, job);
    schedule(id, job);
  }
  return { enqueue, forget, reset };
}
