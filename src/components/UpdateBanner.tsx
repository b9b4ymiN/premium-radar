import { useEffect, useRef, useState } from "react";
import type { Update } from "@tauri-apps/plugin-updater";

/** The banner's lifecycle — hidden until a newer version is found. */
type BannerState = "hidden" | "available" | "installing" | "error";

const CHECK_DELAY_MS = 5_000;
const ERR_CHECK = "ตรวจสอบอัปเดตไม่สำเร็จ";
const ERR_INSTALL = "อัปเดตไม่สำเร็จ กรุณาลองใหม่ภายหลัง";

/**
 * Slim bar under the app bar announcing a newer version (desktop app only —
 * the browser preview renders nothing). The check runs once, five seconds
 * after start; any failure just shows a short Thai message that can be
 * dismissed, so it never blocks the app.
 */
export default function UpdateBanner() {
  const [state, setState] = useState<BannerState>("hidden");
  const [version, setVersion] = useState("");
  const [pct, setPct] = useState<number | null>(null);
  const [error, setError] = useState(ERR_CHECK);
  const updateRef = useRef<Update | null>(null);

  useEffect(() => {
    const inDesktop = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
    if (!inDesktop) return;
    let cancelled = false;
    const timer = setTimeout(async () => {
      try {
        const { check } = await import("@tauri-apps/plugin-updater");
        const update = await check();
        if (cancelled || !update?.available) return;
        updateRef.current = update;
        setVersion(update.version);
        setState("available");
      } catch (e) {
        // a background check that fails (offline, no release yet) must not nag
        // on every launch — log it; install failures are still shown below
        console.warn("update check failed:", e);
      }
    }, CHECK_DELAY_MS);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, []);

  if (state === "hidden") return null;

  const install = async () => {
    setState("installing");
    setPct(null);
    try {
      let update = updateRef.current;
      if (!update) {
        const { check } = await import("@tauri-apps/plugin-updater");
        const found = await check();
        if (found?.available) update = found;
      }
      if (!update) {
        setState("hidden");
        return;
      }
      let total = 0;
      let done = 0;
      await update.downloadAndInstall((event) => {
        switch (event.event) {
          case "Started":
            total = event.data.contentLength ?? 0;
            break;
          case "Progress":
            done += event.data.chunkLength;
            if (total > 0) setPct(Math.min(100, Math.round((done / total) * 100)));
            break;
        }
      });
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    } catch {
      setError(ERR_INSTALL);
      setState("error");
    }
  };

  return (
    <div className="upd-bar" role="status">
      {state === "available" && (
        <>
          <span className="upd-msg">มีเวอร์ชันใหม่ v{version}</span>
          <button className="upd-btn" onClick={() => void install()}>อัปเดตเลย</button>
          <button className="upd-btn ghost" onClick={() => setState("hidden")}>ภายหลัง</button>
        </>
      )}
      {state === "installing" && (
        <>
          <span className="upd-msg">{pct === null ? "กำลังดาวน์โหลดอัปเดต…" : `กำลังอัปเดต… ${pct}%`}</span>
          {pct !== null && (
            <span className="upd-progress">
              <span className="upd-progress-fill" style={{ width: `${pct}%` }} />
            </span>
          )}
        </>
      )}
      {state === "error" && (
        <>
          <span className="upd-msg err">{error}</span>
          <button className="upd-btn ghost" onClick={() => setState("hidden")}>ภายหลัง</button>
        </>
      )}
    </div>
  );
}
