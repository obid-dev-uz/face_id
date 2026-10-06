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
    expect(r && (r as { meta?: { parent?: string } }).meta).toEqual({ parent: "/persons" });
  });
});
