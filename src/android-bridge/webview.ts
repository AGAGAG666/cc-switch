/** Android WebView zoom is applied to the document rather than a Tauri window. */
export function getCurrentWebview() {
  return {
    async setZoom(zoom: number): Promise<void> {
      document.documentElement.style.zoom = String(zoom);
    },
  };
}
