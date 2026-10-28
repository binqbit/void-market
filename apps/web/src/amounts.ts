/** Format a decimal amount without converting it to a floating-point number. */
export function formatZec(value: string): string {
  const match = /^(\d+)(?:\.(\d+))?$/.exec(value);
  if (!match) return value;

  const whole = match[1].replace(/^0+(?=\d)/, "");
  const fraction = (match[2] ?? "").replace(/0+$/, "");
  return `${whole}.${fraction || "0"}`;
}

export function normalizeZecInput(value: string): string {
  return value.replace(",", ".");
}

export function zecInputError(value: string, allowZero = false): string | null {
  if (!value) return null;
  if (!/^\d+(?:\.\d{0,8})?$/.test(value)) {
    return "Enter a number with up to 8 decimal places.";
  }
  if (value.endsWith(".")) return "Add the decimal digits after the separator.";
  if (value.length > 32) return "This amount is too large.";

  const [whole, fraction = ""] = value.split(".");
  const units = BigInt(whole) * 100000000n + BigInt(fraction.padEnd(8, "0"));
  if (units > 9223372036854775807n) return "This amount is too large.";
  if (!allowZero && units === 0n) return "Enter an amount greater than zero.";
  return null;
}
