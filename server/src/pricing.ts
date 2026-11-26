// Offers and prices by country zone (Phase 1 plan). The amounts must match the Stripe prices
// whose ids are given in STRIPE_PRICES; prices include VAT (set "Include tax in prices" in Stripe).

export type Offer = "monthly" | "annual" | "lifetime";
export type Zone = 1 | 2 | 3 | 4;
export const OFFERS: Offer[] = ["monthly", "annual", "lifetime"];

/** EUR, tax included. Zone 1 = 100 %, 2 ≈ 70 %, 3 ≈ 45 %, 4 ≈ 30 %. */
export const PRICES: Record<Zone, Record<Offer, number>> = {
  1: { monthly: 9, annual: 79, lifetime: 149 },
  2: { monthly: 6.5, annual: 55, lifetime: 105 },
  3: { monthly: 4, annual: 35, lifetime: 69 },
  4: { monthly: 3, annual: 25, lifetime: 49 },
};

export const LIFETIME_CAP = 500;

const Z2 = "ES PT IT GR CY MT SI HR PL CZ SK HU RO BG EE LV LT RS BA ME MK AL XK UA MD GE AM";
const Z3 = "BR MX AR CO PE CL EC UY PY BO VE CR PA DO GT HN SV NI MA DZ TN EG LY ID TR PH VN TH MY ZA NG KE GH SN CI CM ET TZ UG JO LB IQ AZ KZ UZ MN";
const Z4 = "IN PK BD NP LK MM KH LA";

const ZONES = new Map<string, Zone>();
for (const [list, zone] of [[Z2, 2], [Z3, 3], [Z4, 4]] as const) for (const c of list.split(" ")) ZONES.set(c, zone);

/** Unknown or missing country: zone 1. */
export function zoneOf(country: string | null | undefined): Zone {
  return ZONES.get((country ?? "").toUpperCase()) ?? 1;
}

export function isOffer(v: string | null): v is Offer {
  return v === "monthly" || v === "annual" || v === "lifetime";
}

/** STRIPE_PRICES = {"monthly":{"1":"price_…","2":"price_…",…},"annual":{…},"lifetime":{…}} */
export function priceId(pricesJson: string, offer: Offer, zone: Zone): string | null {
  try {
    const table = JSON.parse(pricesJson) as Record<string, Record<string, string>>;
    return table[offer]?.[String(zone)] ?? table[offer]?.["1"] ?? null;
  } catch {
    return null;
  }
}
