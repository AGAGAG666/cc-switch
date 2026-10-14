import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

let invoke: typeof import("./runtime").invoke;
const fetchMock = vi.fn();

beforeEach(async () => {
  vi.resetModules();
  delete window.__CCS_HOST__;
  delete window.__CCS_SIDECAR__;
  delete window.__CCS_NATIVE__;
  delete window.__ccsResolveFilePick;
  delete window.__ccsResolveFolderPick;
  vi.stubGlobal("fetch", fetchMock);
  fetchMock.mockReset();
  ({ invoke } = await import("./runtime"));
});
afterEach(() => {
  window.dispatchEvent(new Event("pagehide"));
  delete window.__CCS_HOST__;
  delete window.__CCS_SIDECAR__;
  delete window.__CCS_NATIVE__;
  delete window.__ccsResolveFilePick;
  delete window.__ccsResolveFolderPick;
  vi.unstubAllGlobals();
});

function requestId(native: ReturnType<typeof vi.fn>) {
  return native.mock.calls[native.mock.calls.length - 1][0] as string;
}

describe("Android system SQL document picker", () => {
  it("routes SQL file selection to native without sidecar RPC or database import", async () => {
    const native = vi.fn();
    window.__CCS_NATIVE__ = { pickFile: native };
    const selection = invoke<string | null>("open_file_dialog");
    expect(native).toHaveBeenCalledTimes(1);
    expect(requestId(native)).toMatch(/^[a-z0-9-]+$/);
    window.__ccsResolveFilePick?.(
      requestId(native),
      "/cache/private/backup.sql",
      null,
    );
    await expect(selection).resolves.toBe("/cache/private/backup.sql");
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("resolves cancellation as null and allows another selection", async () => {
    const native = vi.fn();
    window.__CCS_NATIVE__ = { pickFile: native };
    const first = invoke("open_file_dialog");
    window.__ccsResolveFilePick?.(requestId(native), null, null);
    await expect(first).resolves.toBeNull();
    const second = invoke("open_file_dialog");
    window.__ccsResolveFilePick?.(requestId(native), null, null);
    await expect(second).resolves.toBeNull();
    expect(native).toHaveBeenCalledTimes(2);
  });

  it("rejects a provider read error rather than treating it as cancellation", async () => {
    const native = vi.fn();
    window.__CCS_NATIVE__ = { pickFile: native };
    const selection = invoke("open_file_dialog");
    window.__ccsResolveFilePick?.(
      requestId(native),
      null,
      "无法读取所选 SQL 文件",
    );
    await expect(selection).rejects.toThrow("无法读取所选 SQL 文件");
  });

  it("does not open multiple system pickers for repeated clicks", async () => {
    const native = vi.fn();
    window.__CCS_NATIVE__ = { pickFile: native };
    const first = invoke("open_file_dialog");
    await expect(invoke("open_file_dialog")).rejects.toThrow(
      "已有 SQL 文件选择",
    );
    expect(native).toHaveBeenCalledTimes(1);
    window.__ccsResolveFilePick?.(requestId(native), null, null);
    await first;
  });

  it("cleans up a synchronous native failure and permits retry", async () => {
    const native = vi.fn().mockImplementationOnce(() => {
      throw new Error("native failure");
    });
    window.__CCS_NATIVE__ = { pickFile: native };
    await expect(invoke("open_file_dialog")).rejects.toThrow(
      "无法打开系统文件选择器",
    );
    const retry = invoke("open_file_dialog");
    window.__ccsResolveFilePick?.(requestId(native), null, null);
    await expect(retry).resolves.toBeNull();
  });

  it("fails explicitly on an older APK missing the native picker", async () => {
    window.__CCS_NATIVE__ = { pickFolder: vi.fn() };
    await expect(invoke("open_file_dialog")).rejects.toThrow(
      "请更新 ZeroTermux-CCS",
    );
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("does not pretend browser-only mode has an Android host", async () => {
    await expect(invoke("open_file_dialog")).rejects.toThrow(
      "需要在新版 ZeroTermux-CCS 内打开",
    );
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("preserves an injected asynchronous host picker", async () => {
    const pickFile = vi.fn().mockResolvedValue("/cache/injected.sql");
    window.__CCS_HOST__ = { pickFile };
    await expect(invoke("open_file_dialog")).resolves.toBe(
      "/cache/injected.sql",
    );
    expect(pickFile).toHaveBeenCalledOnce();
  });

  it("ignores stale or duplicate callbacks for a newer request", async () => {
    const native = vi.fn();
    window.__CCS_NATIVE__ = { pickFile: native };
    const first = invoke("open_file_dialog");
    const oldId = requestId(native);
    window.__ccsResolveFilePick?.(oldId, null, null);
    await first;
    const second = invoke("open_file_dialog");
    const currentId = requestId(native);
    expect(currentId).not.toBe(oldId);
    window.__ccsResolveFilePick?.(oldId, "/cache/stale.sql", null);
    window.__ccsResolveFilePick?.(currentId, "/cache/current.sql", null);
    await expect(second).resolves.toBe("/cache/current.sql");
  });

  it("rejects pending requests when the page closes", async () => {
    const native = vi.fn();
    window.__CCS_NATIVE__ = { pickFile: native };
    const selection = invoke("open_file_dialog");
    const assertion = expect(selection).rejects.toThrow("页面已关闭");
    window.dispatchEvent(new Event("pagehide"));
    await assertion;
  });

  async function selectedCopy(pin = true) {
    const native = vi.fn();
    const beginSqlImport = vi.fn(() => pin);
    const endSqlImport = vi.fn();
    const releaseSqlImport = vi.fn();
    window.__CCS_NATIVE__ = {
      pickFile: native,
      beginSqlImport,
      endSqlImport,
      releaseSqlImport,
    };
    const selection = invoke("open_file_dialog");
    window.__ccsResolveFilePick?.(
      requestId(native),
      "/cache/private/import.sql",
      null,
    );
    await selection;
    window.__CCS_SIDECAR__ = { port: 43123, token: "synthetic-token" };
    return { beginSqlImport, endSqlImport, releaseSqlImport };
  }

  it("pins a selected copy before RPC and unpins after completion", async () => {
    const host = await selectedCopy();
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ ok: true, data: { success: true } }),
    });
    await expect(
      invoke("import_config_from_file", {
        filePath: "/cache/private/import.sql",
      }),
    ).resolves.toEqual({ success: true });
    expect(host.beginSqlImport).toHaveBeenCalledWith(
      "/cache/private/import.sql",
    );
    expect(host.endSqlImport).toHaveBeenCalledWith("/cache/private/import.sql");
    expect(host.beginSqlImport.mock.invocationCallOrder[0]).toBeLessThan(
      fetchMock.mock.invocationCallOrder[0],
    );
    expect(fetchMock.mock.invocationCallOrder[0]).toBeLessThan(
      host.endSqlImport.mock.invocationCallOrder[0],
    );
  });

  it("does not send an import RPC if the native copy cannot be pinned", async () => {
    const host = await selectedCopy(false);
    await expect(
      invoke("import_config_from_file", {
        filePath: "/cache/private/import.sql",
      }),
    ).rejects.toThrow("临时副本已失效");
    expect(fetchMock).not.toHaveBeenCalled();
    expect(host.endSqlImport).not.toHaveBeenCalled();
  });

  it("retains a copy pin when RPC transport completion is unknown", async () => {
    const host = await selectedCopy();
    fetchMock.mockRejectedValue(new Error("synthetic network failure"));
    await expect(
      invoke("import_config_from_file", {
        filePath: "/cache/private/import.sql",
      }),
    ).rejects.toEqual(expect.stringContaining("无法连接"));
    expect(host.endSqlImport).not.toHaveBeenCalled();
  });

  it("unpins after a completed RPC reports invalid SQL", async () => {
    const host = await selectedCopy();
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ ok: false, error: "synthetic invalid SQL" }),
    });
    await expect(
      invoke("import_config_from_file", {
        filePath: "/cache/private/import.sql",
      }),
    ).rejects.toBe("synthetic invalid SQL");
    expect(host.endSqlImport).toHaveBeenCalledWith("/cache/private/import.sql");
  });

  it("requests release on page close and still unpins the in-flight RPC later", async () => {
    const host = await selectedCopy();
    let finish!: (response: unknown) => void;
    fetchMock.mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const importing = invoke("import_config_from_file", {
      filePath: "/cache/private/import.sql",
    });
    await Promise.resolve();
    await Promise.resolve();
    expect(fetchMock).toHaveBeenCalledOnce();
    window.dispatchEvent(new Event("pagehide"));
    expect(host.releaseSqlImport).toHaveBeenCalledWith(
      "/cache/private/import.sql",
    );
    expect(host.endSqlImport).not.toHaveBeenCalled();
    finish({
      ok: true,
      status: 200,
      json: async () => ({ ok: true, data: { success: true } }),
    });
    await importing;
    expect(host.endSqlImport).toHaveBeenCalledWith("/cache/private/import.sql");
  });

  it("completes missing native picker capabilities without replacing legacy host methods", async () => {
    const native = vi.fn();
    const pickFolder = vi.fn();
    window.__CCS_HOST__ = { pickFolder };
    window.__CCS_NATIVE__ = { pickFile: native };
    const selected = invoke("open_file_dialog");
    expect(native).toHaveBeenCalledOnce();
    expect(window.__CCS_HOST__?.pickFolder).toBe(pickFolder);
    window.__ccsResolveFilePick?.(requestId(native), null, null);
    await selected;
  });

  it("completes native pin capabilities if HOST is replaced after picking", async () => {
    const host = await selectedCopy();
    window.__CCS_HOST__ = { openUrl: vi.fn() };
    fetchMock.mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ ok: true, data: { success: true } }),
    });
    await invoke("import_config_from_file", {
      filePath: "/cache/private/import.sql",
    });
    expect(host.beginSqlImport).toHaveBeenCalledWith(
      "/cache/private/import.sql",
    );
    expect(host.endSqlImport).toHaveBeenCalledWith("/cache/private/import.sql");
  });

  it("keeps directory selection separate and unchanged", async () => {
    const pickFolder = vi.fn().mockResolvedValue("/private/config");
    const pickFile = vi.fn();
    window.__CCS_HOST__ = { pickFolder, pickFile };
    await expect(invoke("pick_directory")).resolves.toBe("/private/config");
    expect(pickFile).not.toHaveBeenCalled();
  });
});
