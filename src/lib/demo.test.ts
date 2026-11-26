import { describe, expect, it } from "vitest";
import { wrapText } from "./demo";

const measure = (s: string) => s.length * 10;

describe("wrapText", () => {
  it("keeps short text on one line", () => {
    expect(wrapText(measure, "Bonjour  le monde", 300, 3)).toEqual(["Bonjour le monde"]);
  });
  it("wraps on words", () => {
    expect(wrapText(measure, "un deux trois quatre", 100, 3)).toEqual(["un deux", "trois", "quatre"]);
  });
  it("ends with an ellipsis when there is too much", () => {
    const lines = wrapText(measure, "un deux trois quatre cinq six sept", 100, 2);
    expect(lines).toHaveLength(2);
    expect(lines[1].endsWith("…")).toBe(true);
    expect(measure(lines[1])).toBeLessThanOrEqual(100);
  });
  it("never loops on a single long word", () => {
    expect(wrapText(measure, "anticonstitutionnellement", 50, 1)[0].endsWith("…")).toBe(true);
  });
});
