/**
 * `@tauri-apps/plugin-dialog` 的 Android 替身。
 *
 * 只有 `message` 有用点（配置加载失败时的致命提示，随后 exit(1)）。
 * WebView 的 `window.alert` 需要宿主实现 `onJsAlert` 才会显示，不能依赖；
 * 这里自己贴一层 DOM 模态，保证提示一定可见、且 await 到用户确认才继续。
 */
export type MessageDialogKind = "info" | "warning" | "error";

export interface MessageDialogOptions {
  title?: string;
  kind?: MessageDialogKind;
  okLabel?: string;
}

const KIND_COLOR: Record<MessageDialogKind, string> = {
  info: "#2563eb",
  warning: "#d97706",
  error: "#dc2626",
};

export async function message(
  text: string,
  options?: MessageDialogOptions | string,
): Promise<void> {
  const opts: MessageDialogOptions =
    typeof options === "string" ? { title: options } : (options ?? {});
  const kind = opts.kind ?? "info";

  return new Promise<void>((resolve) => {
    const mask = document.createElement("div");
    mask.setAttribute("role", "alertdialog");
    mask.setAttribute("aria-modal", "true");
    mask.style.cssText = [
      "position:fixed",
      "inset:0",
      "z-index:2147483647",
      "background:rgba(0,0,0,.55)",
      "display:flex",
      "align-items:center",
      "justify-content:center",
      "padding:16px",
      "font:14px/1.6 system-ui,sans-serif",
    ].join(";");

    const card = document.createElement("div");
    card.style.cssText = [
      "max-width:min(92vw,520px)",
      "max-height:80vh",
      "overflow:auto",
      "background:#fff",
      "color:#111",
      "border-radius:12px",
      "padding:20px",
      "box-shadow:0 12px 40px rgba(0,0,0,.35)",
    ].join(";");

    const heading = document.createElement("div");
    heading.textContent = opts.title ?? "cc-switch";
    heading.style.cssText = `font-weight:600;font-size:16px;margin-bottom:10px;color:${KIND_COLOR[kind]}`;
    mask.setAttribute("aria-label", heading.textContent);

    const body = document.createElement("div");
    body.textContent = text;
    body.style.cssText = "white-space:pre-wrap;word-break:break-word";

    const ok = document.createElement("button");
    ok.type = "button";
    ok.textContent = opts.okLabel ?? "OK";
    ok.style.cssText = [
      "margin-top:18px",
      "min-height:44px",
      "width:100%",
      "border:0",
      "border-radius:8px",
      `background:${KIND_COLOR[kind]}`,
      "color:#fff",
      "font-size:15px",
    ].join(";");

    function done(): void {
      mask.remove();
      resolve();
    }
    ok.addEventListener("click", done);
    mask.addEventListener("keydown", (e) => {
      if ((e as KeyboardEvent).key === "Escape") done();
    });

    card.append(heading, body, ok);
    mask.append(card);
    document.body.append(mask);
    ok.focus();
  });
}

/** 上游没有用点，保留同签名。确认框在 Android 上退化为「取消」以避免误操作。 */
export async function ask(
  text: string,
  options?: MessageDialogOptions,
): Promise<boolean> {
  await message(text, options);
  return false;
}

export async function confirm(
  text: string,
  options?: MessageDialogOptions,
): Promise<boolean> {
  await message(text, options);
  return false;
}

/** 目录/文件选择由宿主负责（sidecar 的 pick_folder 返回 None）。 */
export async function open(_options?: unknown): Promise<string | null> {
  const { pickFolder } = await import("./runtime");
  return pickFolder();
}

export async function save(_options?: unknown): Promise<string | null> {
  return null;
}
