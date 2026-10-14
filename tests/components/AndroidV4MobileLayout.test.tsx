import fs from "node:fs";
import path from "node:path";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { AppPageHeader } from "@/components/shell/AppPageHeader";
import { usesNarrowSidebarDrawer } from "@/hooks/useSidebarCollapsed";
import { SegmentedControl } from "@/components/ui/segmented-control";
import {
  SettingsRow,
  SettingsSwitchRow,
} from "@/components/settings/SettingsLayout";

const source = (file: string) =>
  fs.readFileSync(path.resolve(__dirname, "../../src", file), "utf8");

describe("Android V4 scoped layout", () => {
  it("does not change desktop narrow-window sidebar behavior", () => {
    const media = vi
      .spyOn(window, "matchMedia")
      .mockReturnValue({ matches: true } as MediaQueryList);
    const style = document.createElement("style");
    style.dataset.ccs = "mobile-overrides";
    try {
      expect(usesNarrowSidebarDrawer()).toBe(false);
      document.head.append(style);
      expect(usesNarrowSidebarDrawer()).toBe(true);
    } finally {
      style.remove();
      media.mockRestore();
    }
  });

  it("marks the segmented track without changing selection callbacks", () => {
    const change = vi.fn();
    render(
      <SegmentedControl
        size="sm"
        aria-label="synthetic segments"
        value="a"
        onValueChange={change}
        items={[
          { value: "a", label: "One" },
          { value: "b", label: "Two" },
        ]}
      />,
    );
    expect(
      screen.getByRole("group", { name: "synthetic segments" }),
    ).toHaveAttribute("data-ccs-segmented", "sm");
    expect(screen.getByRole("button", { name: "One" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    fireEvent.click(screen.getByRole("button", { name: "Two" }));
    expect(change).toHaveBeenCalledWith("b");
  });

  it("separates settings labels and controls for mobile reflow", () => {
    render(
      <SettingsRow
        label="Synthetic address"
        control={<input aria-label="synthetic field" />}
      />,
    );
    expect(
      screen.getByText("Synthetic address").closest(".ccs-settings-row-label"),
    ).not.toBeNull();
    expect(
      screen.getByRole("textbox").closest(".ccs-settings-row-control"),
    ).not.toBeNull();
  });

  it("keeps switch rows distinct from stacked form rows", () => {
    render(
      <SettingsSwitchRow
        label="Synthetic switch"
        checked
        onCheckedChange={() => {}}
      />,
    );
    expect(
      screen.getByRole("switch").closest(".ccs-settings-switch-row"),
    ).not.toBeNull();
  });

  it("marks header title and actions independently", () => {
    render(
      <AppPageHeader
        title="Synthetic title"
        actions={<button>Synthetic action</button>}
      />,
    );
    expect(screen.getByRole("banner")).toHaveClass("ccs-page-header");
    expect(
      screen.getByRole("heading").closest(".ccs-page-header-title"),
    ).not.toBeNull();
    expect(
      screen.getByRole("button").closest(".ccs-page-header-actions"),
    ).not.toBeNull();
  });

  it("gives provider identity and tools independent mobile rows", () => {
    const card = source("components/providers/ProviderCard.tsx");
    expect(card).toContain("ccs-provider-card-row");
    expect(card).toContain("ccs-provider-card-identity");
    expect(card).toContain("ccs-provider-card-tools");
  });

  it("uses dedicated WebDAV and session hooks instead of global flex rewrites", () => {
    expect(source("components/settings/WebdavSyncSection.tsx")).toContain(
      "ccs-webdav-field",
    );
    expect(source("components/sessions/SessionManagerPage.tsx")).toContain(
      "ccs-sessions-toolbar",
    );
    expect(source("components/sessions/SessionManagerPage.tsx")).toContain(
      "ccs-sessions-search",
    );
  });

  it("keeps a stable sidebar rail while the narrow drawer expands", () => {
    const sidebar = source("components/shell/Sidebar.tsx");
    expect(sidebar).toContain("ccs-sidebar-container");
    expect(sidebar).toContain("data-collapsed={collapsed}");
    expect(sidebar).toContain("ccs-sidebar-backdrop");
  });
});
