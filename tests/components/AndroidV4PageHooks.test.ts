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
        const scope = /^\.(ccs-(?:usage|auth|apps|import)-[\w-]+)/.exec(
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

  it("registers the three pages and all auth providers without account data", () => {
    expect(hooks[0].data.get("data-ccs-page")).toEqual(["usage"]);
    expect(hooks[3].data.get("data-ccs-page")).toEqual(["auth"]);
    expect(hooks[4].data.get("data-ccs-page")).toEqual(["apps"]);
    expect(hooks[3].data.get("data-ccs-auth-provider")).toEqual([
      "copilot",
      "codex",
      "xai",
    ]);
  });

  it("keeps the shared auth DOM selectors anchored to their current components", () => {
    const group = attributes(
      "src/components/settings/auth/ManagedAccountsGroup.tsx",
    );
    const quota = attributes("src/components/settings/auth/AccountQuota.tsx");
    expect(group.classes.has("min-h-[56px]")).toBe(true);
    for (const width of ["w-[212px]", "w-[272px]", "w-[84px]"]) {
      expect(quota.classes.has(width), width).toBe(true);
    }
  });
});
