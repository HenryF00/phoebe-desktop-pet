const usageKeys = ["input", "output", "cacheRead", "cacheWrite", "totalTokens"];

export function emptyTokenUsage() {
  return { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, totalTokens: 0 };
}

export function normalizeTokenUsage(value) {
  const normalized = emptyTokenUsage();
  for (const key of usageKeys) {
    const number = Number(value?.[key]);
    normalized[key] = Number.isFinite(number) && number > 0 ? Math.trunc(number) : 0;
  }
  return normalized;
}

export function addTokenUsage(total, next) {
  const left = normalizeTokenUsage(total);
  const right = normalizeTokenUsage(next);
  return Object.fromEntries(usageKeys.map(key => [key, left[key] + right[key]]));
}
