import fs from "node:fs";
import path from "node:path";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { VisibleApps } from "@/types";
import { TooltipProvider } from "@/components/ui/tooltip";
import {
  Select,
  SelectTrigger,
  SelectContent,
  SelectItem,
  SelectValue,
} from "@/components/ui/select";
import { Dialog, DialogContent, DialogTitle } from "@/components/ui/dialog";
import { hasOpenNarrowSidebarDrawer } from "@/hooks/useSidebarCollapsed";
vi.mock("@/contexts/UpdateContext", () => ({
  useUpdate: () => ({ hasUpdate: false }),
}));
vi.mock("@/hooks/useSidebarStatus", () => ({
  useSidebarStatus: () => ({
    appStatus: () => ({ mode: "direct", mapping: false, alert: false }),
    todayCost: 0,
    authNeedsAttention: false,
  }),
}));
import { Sidebar } from "@/components/shell/Sidebar";

let narrow = true;
let marker: HTMLStyleElement;
let pref: string | null;
let scrollDescriptor: PropertyDescriptor | undefined;
const media = (query: string) =>
  ({
    matches: query.includes("640px") ? narrow : false,
    media: query,
    onchange: null,
    addListener() {},
    removeListener() {},
    addEventListener() {},
    removeEventListener() {},
    dispatchEvent: () => true,
  }) as MediaQueryList;
const visibleApps: VisibleApps = {
  claude: false,
  "claude-desktop": false,
  codex: true,
  gemini: false,
  grokbuild: false,
  opencode: false,
  openclaw: false,
  hermes: false,
  pi: false,
  mcode: false,
};
function frame(modal = false, selectOpen = false) {
  return (
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <TooltipProvider>
        <Sidebar
          activeApp="codex"
          view="providers"
          visibleApps={visibleApps}
          settingsSection="general"
          onSelectApp={() => {}}
          onSelectPage={() => {}}
          onSelectSettingsSection={() => {}}
          onExitSettings={() => {}}
        />
        <main id="content-area">
          <input aria-label="Synthetic background" />
          <Select open={selectOpen} defaultValue="a">
            <SelectTrigger aria-label="Synthetic language">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="a">Synthetic option A</SelectItem>
              <SelectItem value="b">Synthetic option B</SelectItem>
            </SelectContent>
          </Select>
        </main>
        <Dialog open={modal}>
          <DialogContent aria-describedby={undefined}>
            <DialogTitle>Synthetic modal</DialogTitle>
            <button>synthetic modal action</button>
          </DialogContent>
        </Dialog>
      </TooltipProvider>
    </QueryClientProvider>
  );
}
const toggle = () =>
  document.querySelector<HTMLElement>("[data-ccs-sidebar-toggle]")!;
const nav = () => document.querySelector<HTMLElement>("nav.ccs-sidebar")!;
const open = async () => {
  fireEvent.click(toggle());
  await waitFor(() => expect(nav()).toHaveAttribute("data-collapsed", "false"));
};
beforeEach(() => {
  scrollDescriptor = Object.getOwnPropertyDescriptor(
    Element.prototype,
    "scrollIntoView",
  );
  Object.defineProperty(Element.prototype, "scrollIntoView", {
    configurable: true,
    value: vi.fn(),
  });
  pref = localStorage.getItem("cc-switch-sidebar-collapsed");
  localStorage.setItem("cc-switch-sidebar-collapsed", "true");
  narrow = true;
  vi.spyOn(window, "matchMedia").mockImplementation(media);
  marker = document.createElement("style");
  marker.dataset.ccs = "mobile-overrides";
  document.head.append(marker);
});
afterEach(() => {
  if (scrollDescriptor)
    Object.defineProperty(
      Element.prototype,
      "scrollIntoView",
      scrollDescriptor,
    );
  else Reflect.deleteProperty(Element.prototype, "scrollIntoView");
  marker.remove();
  vi.restoreAllMocks();
  if (pref === null) localStorage.removeItem("cc-switch-sidebar-collapsed");
  else localStorage.setItem("cc-switch-sidebar-collapsed", pref);
});

describe("Android sidebar drawer focus and modal priority", () => {
  it("blocks the drawer shortcut while a real Select listbox is open", async () => {
    render(frame(false, true));
    await screen.findByRole("listbox");
    const option = screen.getByRole("option", { name: "Synthetic option B" });
    option.focus();
    fireEvent.keyDown(window, { key: "\\", ctrlKey: true });
    expect(nav()).toHaveAttribute("data-collapsed", "true");
    expect(document.activeElement).toBe(option);
  });
  it("yields focus to a Select portal opened after the drawer", async () => {
    const view = render(frame());
    await open();
    view.rerender(frame(false, true));
    const option = await screen.findByRole("option", {
      name: "Synthetic option B",
    });
    option.focus();
    expect(document.activeElement).toBe(option);
    view.rerender(frame(false, false));
    await waitFor(() =>
      expect(nav().contains(document.activeElement)).toBe(true),
    );
  });

  it("contains Tab and background focus; Escape restores the toggle", async () => {
    render(frame());
    await open();
    expect(document.getElementById("content-area")).toHaveAttribute("inert");
    const nodes = [
      ...nav().querySelectorAll<HTMLElement>(
        "button:not([disabled]),a[href],[tabindex]",
      ),
    ].filter((e) => e.tabIndex >= 0 && getComputedStyle(e).display !== "none");
    nodes[nodes.length - 1].focus();
    fireEvent.keyDown(window, { key: "Tab" });
    expect(document.activeElement).toBe(nodes[0]);
    fireEvent.keyDown(window, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(nodes[nodes.length - 1]);
    screen.getByLabelText("Synthetic background").focus();
    expect(nav().contains(document.activeElement)).toBe(true);
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() =>
      expect(nav()).toHaveAttribute("data-collapsed", "true"),
    );
    expect(document.getElementById("content-area")).not.toHaveAttribute(
      "inert",
    );
    expect(document.activeElement).toBe(toggle());
  });
  it("blocks the global drawer shortcut while a real modal portal is open", async () => {
    render(frame(true));
    await screen.findByRole("dialog");
    const modalButton = screen.getByRole("button", {
      name: "synthetic modal action",
    });
    modalButton.focus();
    fireEvent.keyDown(window, { key: "\\", ctrlKey: true });
    expect(document.activeElement).toBe(modalButton);
    expect(nav()).toHaveAttribute("data-collapsed", "true");
    expect(document.getElementById("content-area")).not.toHaveAttribute(
      "inert",
    );
  });
  it("yields focus to a portal opened after the drawer", async () => {
    const view = render(frame());
    await open();
    view.rerender(frame(true));
    const button = await screen.findByRole("button", {
      name: "synthetic modal action",
    });
    button.focus();
    expect(document.activeElement).toBe(button);
    view.rerender(frame(false));
    await waitFor(() =>
      expect(nav().contains(document.activeElement)).toBe(true),
    );
  });
  it("lets the drawer consume Escape before the settings navigation handler", async () => {
    const leave = vi.fn();
    const handler = (event: KeyboardEvent) => {
      if (
        event.key === "Escape" &&
        !event.defaultPrevented &&
        !hasOpenNarrowSidebarDrawer()
      ) {
        event.preventDefault();
        leave();
      }
    };
    window.addEventListener("keydown", handler);
    try {
      render(frame());
      await open();
      fireEvent.keyDown(window, { key: "Escape" });
      await waitFor(() =>
        expect(nav()).toHaveAttribute("data-collapsed", "true"),
      );
      expect(leave).not.toHaveBeenCalled();
    } finally {
      window.removeEventListener("keydown", handler);
    }
    expect(
      fs.readFileSync(path.resolve(__dirname, "../../src/App.tsx"), "utf8"),
    ).toContain("if (hasOpenNarrowSidebarDrawer()) return;");
  });
  it("releases the background when resizing out of mobile drawer mode", async () => {
    render(frame());
    await open();
    narrow = false;
    fireEvent(window, new Event("resize"));
    await waitFor(() =>
      expect(document.getElementById("content-area")).not.toHaveAttribute(
        "inert",
      ),
    );
  });
});
