import { describe, expect, it, vi } from "vitest";
import { takePendingAndroidDeepLink } from "./deeplink";

describe("takePendingAndroidDeepLink", () => {
  it("returns a pending custom-scheme URL without logging its query", () => {
    const url =
      "ccswitch://v1/import?resource=provider&app=codex&apiKey=sk-test-placeholder";
    expect(takePendingAndroidDeepLink({ takePendingDeepLink: () => url })).toBe(
      url,
    );
  });

  it("treats empty or non-string bridge values as no pending link", () => {
    expect(
      takePendingAndroidDeepLink({ takePendingDeepLink: () => "   " }),
    ).toBeNull();
    expect(
      takePendingAndroidDeepLink({ takePendingDeepLink: () => 42 }),
    ).toBeNull();
    expect(takePendingAndroidDeepLink(undefined)).toBeNull();
  });

  it("contains bridge failures instead of breaking the dialog", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    expect(
      takePendingAndroidDeepLink({
        takePendingDeepLink: () => {
          throw new Error("bridge unavailable");
        },
      }),
    ).toBeNull();
    expect(warn).toHaveBeenCalledTimes(1);
    warn.mockRestore();
  });
});
