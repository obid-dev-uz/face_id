import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";

/* Android: sahifa manzili aniqlanguncha Tauri chaqiruvlarni "not allowed by ACL" bilan rad etadi.
 * waitForIpc bitta chaqiruv o'tguncha kutadi (isTauri import paytida hisoblanadi — qayta import). */
async function load() {
  vi.resetModules();
  return (await import("../ipcReady")).waitForIpc;
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  clearMocks();
  vi.useRealTimers();
});

describe("waitForIpc", () => {
  it("waits until calls stop being rejected by ACL", async () => {
    let calls = 0;
    mockIPC(() => {
      if (++calls <= 3) throw "Command plugin:app|version not allowed by ACL";
      return "0.1.0";
    });
    const waitForIpc = await load();
    const done = vi.fn();
    const p = waitForIpc().then(done);
    await vi.advanceTimersByTimeAsync(100);
    expect(done).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(100);
    await p;
    expect(calls).toBe(4);
  });

  it("does not wait on other errors", async () => {
    let calls = 0;
    mockIPC(() => {
      calls++;
      throw "boshqa xato";
    });
    const waitForIpc = await load();
    await waitForIpc();
    expect(calls).toBe(1);
  });

  it("gives up after the time limit", async () => {
    let calls = 0;
    mockIPC(() => {
      calls++;
      throw "Command x not allowed by ACL";
    });
    const waitForIpc = await load();
    const p = waitForIpc();
    await vi.advanceTimersByTimeAsync(5000);
    await p;
    expect(calls).toBeGreaterThan(50);
    expect(calls).toBeLessThan(70);
  });
});
