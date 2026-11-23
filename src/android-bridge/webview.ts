/** Android WebView zoom is applied to the document rather than a Tauri window. */
export function getCurrentWebview() {
  return {
    async setZoom(zoom: number): Promise<void> {
      const style = document.documentElement.style;
      style.zoom = String(zoom);
      style.setProperty("--ccs-page-zoom", String(zoom));
    },
  };
}
