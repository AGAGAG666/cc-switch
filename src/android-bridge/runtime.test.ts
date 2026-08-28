import { describe, expect, it } from "vitest";
import { resolveSidecarBaseUrl } from "./runtime";

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
