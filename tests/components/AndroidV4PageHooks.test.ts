import fs from "node:fs";
import path from "node:path";
import postcss from "postcss";
import ts from "typescript";
import { describe, expect, it } from "vitest";

const root = path.resolve(__dirname, "../..");
const css = postcss.parse(
  fs.readFileSync(
    path.join(root, "src/android-bridge/mobile-v4-pages.css"),
    "utf8",
  ),
);
const pages = [
  "src/components/usage/UsageDashboard.tsx",
  "src/components/usage/UsageHero.tsx",
  "src/components/usage/UsageTrendChart.tsx",
  "src/components/settings/AuthCenterPanel.tsx",
  "src/components/apps/AppsPage.tsx",
  "src/components/settings/ImportExportSection.tsx",
  "src/components/providers/forms/ProviderPresetSelector.tsx",
];

function attributes(file: string) {
  const source = ts.createSourceFile(
    file,
    fs.readFileSync(path.join(root, file), "utf8"),
    ts.ScriptTarget.Latest,
    true,
    ts.ScriptKind.TSX,
  );
  const classes = new Set<string>();
  const data = new Map<string, string[]>();
  function visit(node: ts.Node) {
    if (ts.isJsxAttribute(node)) {
      const name = node.name.getText(source);
      if (name === "className") {
        function collect(value: ts.Node) {
          if (ts.isStringLiteral(value)) {
            value.text.split(/\s+/).forEach((entry) => classes.add(entry));
          }
          ts.forEachChild(value, collect);
        }
        if (node.initializer) collect(node.initializer);
      } else if (
        name.startsWith("data-ccs-") &&
        node.initializer &&
        ts.isStringLiteral(node.initializer)
      ) {
        data.set(name, [...(data.get(name) ?? []), node.initializer.text]);
      }
    }
    ts.forEachChild(node, visit);
  }
  visit(source);
  return { classes, data };
}

const hooks = pages.map(attributes);
const classes = new Set(hooks.flatMap((page) => [...page.classes]));

describe("Android V4 page CSS boundary", () => {
  it("keeps every override inside the 640px mobile breakpoint", () => {
    const media = css.nodes.filter((node) => node.type !== "comment");
    expect(media).toHaveLength(1);
    expect(media[0].type).toBe("atrule");
    expect(media[0]).toMatchObject({
      name: "media",
      params: "(max-width: 640px)",
    });
    css.walkRules((rule) => {
      expect(rule.parent).toBe(media[0]);
    });
  });

  it("scopes every selector to a registered page hook", () => {
    css.walkRules((rule) => {
      // Split selector lists, preserving commas inside :has() and :not().
      for (const selector of rule.selectors) {
        const scope = /^\.(ccs-(?:usage|auth|apps|import|preset)-[\w-]+)/.exec(
          selector,
        );
        expect(scope, selector).not.toBeNull();
        expect(classes.has(scope![1]), selector).toBe(true);
        expect(selector).not.toMatch(/(?:^|[ >+~])\.flex(?:[ :>+~]|$)/);
      }
    });
  });

  it("does not scale the UI or hide page overflow", () => {
    css.walkDecls((declaration) => {
      expect(["zoom", "transform", "scale", "font-size"]).not.toContain(
        declaration.prop,
      );
      if (declaration.prop.startsWith("overflow")) {
        if (declaration.value === "hidden") {
          expect(declaration.parent).toMatchObject({ type: "rule" });
          expect((declaration.parent as postcss.Rule).selector).toContain(
            '[role="meter"]',
          );
        }
      }
    });
  });

  it("registers every page root and the shared auth section hook", () => {
    expect(hooks[0].data.get("data-ccs-page")).toEqual(["usage"]);
    expect(hooks[3].data.get("data-ccs-page")).toEqual(["auth"]);
    expect(hooks[4].data.get("data-ccs-page")).toEqual(["apps"]);
    expect(hooks[6].data.get("data-ccs-page")).toEqual(["presets"]);
    expect(hooks[3].classes.has("ccs-auth-section")).toBe(true);
  });

  it("keeps the shared auth DOM selectors anchored to their current components", () => {
    const group = attributes(
      "src/components/settings/auth/ManagedAccountsGroup.tsx",
    );
    const quota = attributes("src/components/settings/auth/AccountQuota.tsx");
    for (const hook of ["auth-account-row", "auth-account-quota"]) {
      expect(group.classes.has(hook), hook).toBe(true);
    }
    for (const hook of ["auth-quota-col", "auth-quota-refresh"]) {
      expect(quota.classes.has(hook), hook).toBe(true);
    }
  });

  it("anchors the v4.0.38 usage and apps overrides to their real DOM owners", () => {
    const expected = new Map([
      [
        "src/components/usage/UsageDashboard.tsx",
        [
          "ccs-usage-page",
          "ccs-usage-body",
          "ccs-usage-filters",
          "ccs-usage-filter-spacer",
          "ccs-usage-filter-trigger",
          "ccs-usage-app-filter",
          "ccs-usage-tabbar",
          "ccs-usage-tabpanel",
        ],
      ],
      [
        "src/components/usage/UsageDateRangePicker.tsx",
        ["ccs-usage-filter-trigger"],
      ],
      [
        "src/components/usage/UsageHero.tsx",
        [
          "ccs-usage-hero",
          "ccs-usage-metric-grid",
          "ccs-usage-metric",
          "ccs-usage-mini-metric",
          "ccs-usage-metric-label",
          "ccs-usage-metric-value",
          "ccs-usage-more",
        ],
      ],
      [
        "src/components/usage/UsageTrendChart.tsx",
        ["ccs-usage-trend", "ccs-usage-trend-title", "ccs-usage-trend-canvas"],
      ],
      [
        "src/components/apps/AppsPage.tsx",
        [
          "ccs-apps-page",
          "ccs-apps-content",
          "ccs-apps-columns",
          "ccs-apps-column-app",
          "ccs-apps-column-version",
          "ccs-apps-column-action",
          "ccs-apps-column-visibility",
          "ccs-apps-column-menu",
          "ccs-apps-row",
          "ccs-apps-row-grid",
          "ccs-apps-row-main",
          "ccs-apps-info",
          "ccs-apps-name",
          "ccs-apps-meta",
          "ccs-apps-version",
          "ccs-apps-action",
          "ccs-apps-visibility",
          "ccs-apps-menu",
          "ccs-apps-footer",
        ],
      ],
      [
        "src/components/settings/AuthCenterPanel.tsx",
        ["ccs-auth-center", "ccs-auth-browser-toggle", "ccs-auth-section"],
      ],
    ]);

    for (const [file, hooks] of expected) {
      const owner = attributes(file);
      for (const hook of hooks) {
        expect(owner.classes.has(hook), `${file}: ${hook}`).toBe(true);
      }
    }

    const selectors = new Set<string>();
    css.walkRules((rule) =>
      rule.selectors.forEach((selector) => selectors.add(selector)),
    );
    for (const removed of [
      ".ccs-usage-hero-row",
      ".ccs-usage-metrics",
      ".ccs-usage-trend-spacer",
      ".ccs-usage-trend-header",
      ".ccs-usage-trend-controls",
      ".ccs-apps-list",
    ]) {
      expect(selectors.has(removed), removed).toBe(false);
    }
  });
});

function mobileDeclarations(selector: string) {
  const result: Record<string, string> = {};
  css.walkRules((rule) => {
    if (!rule.selectors.includes(selector)) return;
    rule.walkDecls((declaration) => {
      result[declaration.prop] = declaration.value;
    });
  });
  return result;
}

describe("Android provider preset picker layout", () => {
  it("stacks categories above results with one accessible scroll area", () => {
    expect(mobileDeclarations(".ccs-preset-body")).toMatchObject({
      "flex-direction": "column",
      "overflow-y": "auto",
    });
    expect(mobileDeclarations(".ccs-preset-categories")).toMatchObject({
      display: "grid",
      width: "100%",
      flex: "none",
      "overflow-y": "visible",
    });
    expect(mobileDeclarations(".ccs-preset-results")).toMatchObject({
      width: "100%",
      flex: "none",
      "overflow-y": "visible",
    });
  });

  it("lets cards and their text grow without clipping routing badges", () => {
    expect(
      mobileDeclarations(".ccs-preset-grid")["grid-template-columns"],
    ).toBe("repeat(auto-fit, minmax(min(100%, 16rem), 1fr))");
    expect(mobileDeclarations(".ccs-preset-row")).toMatchObject({
      display: "grid",
      height: "auto",
      "grid-template-columns": "32px minmax(0, 1fr)",
    });
    for (const hook of ["category-label", "name", "detail"]) {
      expect(mobileDeclarations(`.ccs-preset-${hook}`)).toMatchObject({
        "white-space": "normal",
        overflow: "visible",
        "overflow-wrap": "anywhere",
      });
    }
    expect(
      mobileDeclarations(".ccs-preset-row > .ccs-preset-route-badge"),
    ).toMatchObject({
      "grid-column": "2",
      "white-space": "normal",
    });
    expect(mobileDeclarations(".ccs-preset-manage")["grid-column"]).toBe(
      "1 / -1",
    );
  });

  it("preserves the original desktop dimensions and hooks every grid branch", () => {
    expect(hooks[6].classes.has("w-[168px]")).toBe(true);
    expect(hooks[6].classes.has("grid-cols-2")).toBe(true);
    const source = fs.readFileSync(path.join(root, pages[6]), "utf8");
    expect(
      source.match(/ccs-preset-grid grid grid-cols-2 gap-2/g),
    ).toHaveLength(4);
    expect(mobileDeclarations(".ccs-preset-category")["min-height"]).toBe(
      "40px",
    );
  });
});
