import { describe, expect, it, vi } from "vitest";

import { ImageProxyBroker } from "@/features/image-treatment/image-proxy-broker";

describe("ImageProxyBroker", () => {
  it("coalesces inspector and canvas consumers and releases independently", async () => {
    const load = vi.fn(async () => ({ bytes: 6 }));
    const broker = new ImageProxyBroker<{ bytes: number }>(
      10,
      (value) => value.bytes,
    );
    const inspector = broker.acquire("same-proxy", load);
    const canvas = broker.acquire("same-proxy", load);

    await expect(Promise.all([inspector.value, canvas.value])).resolves.toEqual([
      { bytes: 6 },
      { bytes: 6 },
    ]);
    expect(load).toHaveBeenCalledTimes(1);
    inspector.release();
    expect(broker.snapshot().activeReferences).toBe(1);
    canvas.release();
    expect(broker.snapshot().activeReferences).toBe(0);
  });

  it("uses a byte budget, retries failures and never evicts active consumers", async () => {
    const broker = new ImageProxyBroker<{ bytes: number }>(
      10,
      (value) => value.bytes,
    );
    const active = broker.acquire("active", async () => ({ bytes: 8 }));
    await active.value;
    const overflow = broker.acquire("overflow", async () => ({ bytes: 8 }));
    await overflow.value;
    overflow.release();
    expect(broker.snapshot().entries).toBe(1);
    expect(broker.snapshot().activeReferences).toBe(1);
    active.release();

    const failed = broker.acquire("retry", async () => {
      throw new Error("transient");
    });
    await expect(failed.value).rejects.toThrow("transient");
    failed.release();
    const retry = broker.acquire("retry", async () => ({ bytes: 1 }));
    await expect(retry.value).resolves.toEqual({ bytes: 1 });
  });
});

export {};
