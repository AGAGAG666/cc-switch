import { afterEach, describe, expect, it, vi } from "vitest";
import { Channel, invoke, resolveSidecarBaseUrl } from "./runtime";

const handshake = { port: 43123, token: "test-token" };

describe("resolveSidecarBaseUrl", () => {
  it("keeps localhost browser requests same-origin", () => {
    expect(
      resolveSidecarBaseUrl(handshake, new URL("http://localhost:43123/")),
    ).toBe("http://localhost:43123");
  });

  it("keeps 127.0.0.1 WebView requests same-origin", () => {
    expect(
      resolveSidecarBaseUrl(handshake, new URL("http://127.0.0.1:43123/")),
    ).toBe("http://127.0.0.1:43123");
  });

  it("does not send the sidecar token to an unrelated origin", () => {
    expect(
      resolveSidecarBaseUrl(handshake, new URL("http://example.test:43123/")),
    ).toBe("http://127.0.0.1:43123");
  });

  it("falls back when the page port is not the handshake port", () => {
    expect(
      resolveSidecarBaseUrl(handshake, new URL("http://localhost:3000/")),
    ).toBe("http://127.0.0.1:43123");
  });
});

describe("Android session transport", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    delete window.__CCS_SIDECAR__;
  });

  it("streams chunks over a token-authenticated POST without putting paths in URL", async () => {
    window.__CCS_SIDECAR__ = { port: 43123, token: "test-token" };
    const chunks: unknown[] = [];
    const channel = new Channel<unknown>();
    channel.onmessage = (chunk) => chunks.push(chunk);
    const stream = new ReadableStream({
      start(controller) {
        controller.enqueue(
          new TextEncoder().encode('data: {"type":"header","total":1}\n\n'),
        );
        controller.enqueue(
          new TextEncoder().encode(
            'data: {"type":"done","payloadBytes":2}\n\n',
          ),
        );
        controller.close();
      },
    });
    // jsdom's Response lacks a WHATWG stream reader; browser fetch provides one.
    const fetchMock = vi.fn(
      async () => ({ ok: true, status: 200, body: stream }) as Response,
    );
    vi.stubGlobal("fetch", fetchMock);
    await invoke("stream_session_messages", {
      providerId: "codex",
      sourcePath: "/private/session.jsonl",
      onChunk: channel,
    });
    expect(chunks).toEqual([
      { type: "header", total: 1 },
      { type: "done", payloadBytes: 2 },
    ]);
    const [url, request] = fetchMock.mock.calls[0] as unknown as [
      string,
      RequestInit,
    ];
    expect(url).toBe("http://127.0.0.1:43123/rpc-stream/session-messages");
    expect(url).not.toContain("session.jsonl");
    expect(request.headers).toMatchObject({ "X-CCS-Token": "test-token" });
    expect(request.body).toContain("/private/session.jsonl");
    expect(request.body).not.toContain("onChunk");
  });

  it("rejects a truncated transcript stream instead of reporting success", async () => {
    window.__CCS_SIDECAR__ = { port: 43123, token: "test-token" };
    const channel = new Channel<unknown>();
    channel.onmessage = vi.fn();
    const stream = new ReadableStream({
      start(controller) {
        controller.enqueue(
          new TextEncoder().encode('data: {"type":"header","total":4}\n\n'),
        );
        controller.close();
      },
    });
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({ ok: true, status: 200, body: stream }) as Response),
    );
    await expect(
      invoke("stream_session_messages", {
        providerId: "codex",
        sourcePath: "/private/session.jsonl",
        onChunk: channel,
      }),
    ).rejects.toThrow("closed before Done");
  });

  it("does not accept a rejected binary request as an image", async () => {
    window.__CCS_SIDECAR__ = { port: 43123, token: "test-token" };
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => new Response(null, { status: 401 })),
    );
    await expect(
      invoke("get_session_image", {
        providerId: "codex",
        sourcePath: "/private/session.jsonl",
        image: {},
      }),
    ).rejects.toThrow("401");
  });

  it("fetches binary session images using the same token", async () => {
    window.__CCS_SIDECAR__ = { port: 43123, token: "test-token" };
    const bytes = Uint8Array.from([0, 255, 42]);
    const fetchMock = vi.fn(async () => new Response(bytes, { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    const result = await invoke<ArrayBuffer>("get_session_image", {
      providerId: "codex",
      sourcePath: "/private/session.jsonl",
      image: { source: "inline" },
    });
    expect([...new Uint8Array(result)]).toEqual([0, 255, 42]);
    const [url, request] = fetchMock.mock.calls[0] as unknown as [
      string,
      RequestInit,
    ];
    expect(url).toBe("http://127.0.0.1:43123/rpc-binary/session-image");
    expect(request.headers).toMatchObject({ "X-CCS-Token": "test-token" });
  });
});
