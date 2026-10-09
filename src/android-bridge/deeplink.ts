type NativeDeepLinkSource = {
  takePendingDeepLink?: () => unknown;
};

/**
 * 从 Android 宿主取出一条待处理链接。宿主方法是一次性读取；这里不记录 URL，
 * 避免 query 中的 apiKey 进入控制台。
 */
export function takePendingAndroidDeepLink(
  source: NativeDeepLinkSource | undefined = window.__CCS_NATIVE__,
): string | null {
  try {
    const value = source?.takePendingDeepLink?.();
    if (typeof value !== "string") return null;
    const normalized = value.trim();
    return normalized || null;
  } catch (error) {
    console.warn("[ccs] 无法从 Android 宿主读取 deep link", error);
    return null;
  }
}
