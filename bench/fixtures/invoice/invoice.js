// Invoice totals. Prices in euros, VAT rate in percent.

export function lineTotal(line) {
  return line.unitPrice * line.qty;
}

export function totals(lines, vatRate = 20) {
  const ht = lines.reduce((sum, l) => sum + lineTotal(l), 0);
  const tva = (ht * vatRate) / 100;
  return {
    ht: Math.round(ht * 100) / 100,
    tva: Math.round(tva * 100) / 100,
    ttc: ht + tva,
  };
}
