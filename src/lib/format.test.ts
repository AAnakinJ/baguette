import { describe, expect, it } from "vitest";
import { direction, money, nextAt, percent } from "./format";

const eur = { currency: "eur" as const, usd_to_eur: 0.5 };
const usd = { currency: "usd" as const, usd_to_eur: 0.5 };

describe("money", () => {
  it("converts USD list prices to euros", () => {
    expect(money(2, eur, "fr")).toMatch(/1,00\s?€/);
  });
  it("keeps dollars when asked", () => {
    expect(money(2, usd, "en")).toBe("$2.00");
  });
});

describe("percent", () => {
  it("formats a share of the subscription window", () => {
    expect(percent(11.4, "en")).toBe("11%");
    expect(percent(4.25, "en")).toBe("4.3%");
    expect(percent(-3, "en")).toBe("0%");
  });
});

describe("nextAt", () => {
  it("picks today when the time is still ahead, tomorrow otherwise", () => {
    const evening = new Date(2026, 10, 25, 22, 30).getTime();
    expect(new Date(nextAt("07:00", evening)! * 1000)).toEqual(new Date(2026, 10, 26, 7, 0));
    const morning = new Date(2026, 10, 25, 5, 0).getTime();
    expect(new Date(nextAt("7:00", morning)! * 1000)).toEqual(new Date(2026, 10, 25, 7, 0));
  });
  it("rejects nonsense", () => {
    expect(nextAt("25:00")).toBeNull();
    expect(nextAt("")).toBeNull();
  });
});

describe("direction", () => {
  it("is right-to-left for Arabic only", () => {
    expect(direction("ar")).toBe("rtl");
    expect(direction("fr")).toBe("ltr");
  });
});
