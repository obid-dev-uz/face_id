import { describe, expect, it } from "vitest";
import { backTarget, HOME } from "../composables/useBack";
import { routes } from "../router";

describe("Android back button", () => {
  it("goes to parent, then home, and exits only from home", () => {
    expect(backTarget("/persons/5", "/persons")).toBe("/persons");
    expect(backTarget("/logs", undefined)).toBe(HOME);
    expect(backTarget(HOME, undefined)).toBeNull();
  });

  it("person page declares its parent", () => {
    const r = routes.find((x) => x.path === "/persons/:id");
    expect(r && (r as { meta?: { parent?: string } }).meta).toMatchObject({ parent: "/persons" });
  });
});

import { webviewMajor } from "../platform";

describe("WebView version", () => {
  it("parses Chrome major from Android WebView UA", () => {
    expect(webviewMajor("Mozilla/5.0 (Linux; Android 7.0; wv) AppleWebKit/537.36 Chrome/52.0.2743.100 Mobile Safari/537.36")).toBe(52);
    expect(webviewMajor("Mozilla/5.0 (X11; Linux x86_64) Firefox/120.0")).toBeNull();
  });
});
