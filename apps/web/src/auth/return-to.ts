export function safeReturnTo(value: string | null): string {
  if (
    !value?.startsWith("/") ||
    value.startsWith("//") ||
    value.includes("\\")
  ) {
    return "/portfolio";
  }
  const url = new URL(value, "https://void.invalid");
  const allowed = [
    "/",
    "/market",
    "/portfolio",
    "/messages",
    "/account",
    "/demo",
    "/docs",
  ];
  if (!allowed.includes(url.pathname) && !url.pathname.startsWith("/market/")) {
    return "/portfolio";
  }
  return `${url.pathname}${url.search}${url.hash}`;
}

export function loginHref(destination: string): string {
  return `/login?next=${encodeURIComponent(safeReturnTo(destination))}`;
}
