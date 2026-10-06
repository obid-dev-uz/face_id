import { describe, expect, it } from "vitest";
import uz from "../i18n/uz";
import ru from "../i18n/ru";
import en from "../i18n/en";

function keys(o: Record<string, unknown>, prefix = ""): string[] {
  return Object.keys(o).flatMap((k) => {
    const v = o[k];
    return v && typeof v === "object" ? keys(v as Record<string, unknown>, prefix + k + ".") : [prefix + k];
  }).sort();
}

describe("locales", () => {
  it("have the same keys", () => {
    expect(keys(ru)).toEqual(keys(uz));
    expect(keys(en)).toEqual(keys(uz));
  });
});
