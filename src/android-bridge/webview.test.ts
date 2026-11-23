import { afterEach, expect, it } from "vitest";
import { getCurrentWebview } from "./webview";

afterEach(() => {
  document.documentElement.removeAttribute("style");
});

it("keeps the viewport sizing factor in sync when zooming and resetting", async () => {
  const style = document.documentElement.style;
  for (const level of [0.9, 1.1, 1.25, 1]) {
    await getCurrentWebview().setZoom(level);
    expect(style.zoom).toBe(String(level));
    expect(style.getPropertyValue("--ccs-page-zoom")).toBe(String(level));
  }
});
