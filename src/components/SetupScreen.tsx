/**
 * API key configuration — first-run wizard AND settings editor.
 *
 * - mode="first-run": shown when needs_setup() is true (nothing saved yet).
 *   Offers "ข้าม (ใช้ env vars)" for dev launches from a shell.
 * - mode="edit": opened from the ⚙ button — loads the SAVED settings into the
 *   fields so nothing looks wiped; saving overwrites with the field values.
 *
 * "ทดสอบการเชื่อมต่อ" performs a REAL Webull REST probe with the typed keys
 * (before saving) so 401/403/timeout surface here, not mid-scan.
 */

import { useEffect, useState } from "react";

interface SetupScreenProps {
  mode?: "first-run" | "edit";
  onComplete: () => void;
  onSkip?: () => void; // first-run: use env vars · edit: cancel without saving
}

interface ConnectionTestResult {
  webull_ok: boolean;
  detail: string;
  streaming?: "entitled" | "limit" | "none" | "unknown" | null;
  /** Set only when the configured signing scheme was rejected and the other one worked. */
  signature_scheme?: "v3" | "v2" | null;
}

interface StoredLlm {
  label?: string;
  base_url?: string;
  model?: string;
  has_api_key?: boolean;
}

/** recorder_test_connection result — the token never crosses IPC, only this verdict. */
interface RecorderTestResult {
  ok: boolean;
  version?: string | null;
  dates_count?: number | null;
  last_date?: string | null;
  server_time?: string | null;
  message_th: string;
}

/** get_settings now returns a redacted SettingsView — secrets arrive only
 *  as has_* booleans, never as values. */
interface StoredSettings {
  webull_app_key?: string;
  webull_base_url?: string;
  has_webull_secret?: boolean;
  has_access_token?: boolean;
  llm?: StoredLlm | null;
  lab_source?: "local" | "api";
  recorder_api_url?: string;
  has_recorder_api_token?: boolean;
}

const hasTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export function SetupScreen({ mode = "first-run", onComplete, onSkip }: SetupScreenProps) {
  const [webullKey, setWebullKey] = useState("");
  const [webullSecret, setWebullSecret] = useState("");
  const [webullToken, setWebullToken] = useState("");
  const [webullUrl, setWebullUrl] = useState("");
  const [llmLabel, setLlmLabel] = useState("");
  const [llmUrl, setLlmUrl] = useState("");
  const [llmModel, setLlmModel] = useState("");
  const [llmKey, setLlmKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<ConnectionTestResult | null>(null);
  const [detectedScheme, setDetectedScheme] = useState<"v3" | "v2" | null>(null);
  const [creatingToken, setCreatingToken] = useState(false);
  const [tokenNotice, setTokenNotice] = useState<{ ok: boolean; text: string } | null>(null);
  const [tokenIssued, setTokenIssued] = useState(false);
  const [testingLlm, setTestingLlm] = useState(false);
  const [llmResult, setLlmResult] = useState<ConnectionTestResult | null>(null);
  const [error, setError] = useState("");
  // has_* flags from the redacted SettingsView: an empty secret input then
  // means "keep the saved value" instead of "no value"
  const [hasSecret, setHasSecret] = useState(false);
  const [hasToken, setHasToken] = useState(false);
  const [hasLlmKey, setHasLlmKey] = useState(false);
  // Lab data source (local / recorder server)
  const [labSource, setLabSource] = useState<"local" | "api">("local");
  const [recorderUrl, setRecorderUrl] = useState("");
  const [recorderToken, setRecorderToken] = useState("");
  const [hasRecorderToken, setHasRecorderToken] = useState(false);
  const [testingRecorder, setTestingRecorder] = useState(false);
  const [recorderResult, setRecorderResult] = useState<RecorderTestResult | null>(null);

  const isEdit = mode === "edit";

  // Load the SAVED settings so reopening ⚙ shows what's stored. Secrets never
  // arrive from the backend — their fields start empty and show a "saved —
  // leave blank to keep" placeholder when one exists. The Base URL prefill
  // shows the EFFECTIVE default explicitly.
  useEffect(() => {
    if (!hasTauri) return;
    import("@tauri-apps/api/core")
      .then(({ invoke }) => invoke<StoredSettings>("get_settings"))
      .then((st) => {
        setWebullKey(st?.webull_app_key ?? "");
        setWebullUrl(st?.webull_base_url?.trim() || "https://api.webull.co.th");
        setLlmLabel(st?.llm?.label ?? "");
        setLlmUrl(st?.llm?.base_url ?? "");
        setLlmModel(st?.llm?.model ?? "");
        setHasSecret(!!st?.has_webull_secret);
        setHasToken(!!st?.has_access_token);
        setHasLlmKey(!!st?.llm?.has_api_key);
        setLabSource(st?.lab_source === "api" ? "api" : "local");
        setRecorderUrl(st?.recorder_api_url ?? "");
        setHasRecorderToken(!!st?.has_recorder_api_token);
      })
      .catch(() => {});
  }, []);

  const keptHint = "บันทึกไว้แล้ว — เว้นว่างเพื่อใช้ค่าเดิม";

  const llmPresets = [
    { label: "Z.ai GLM-5.3", url: "https://api.z.ai/api/paas/v4", model: "glm-5.3" },
    { label: "OpenAI GPT-4o", url: "https://api.openai.com", model: "gpt-4o" },
    { label: "Ollama (local)", url: "http://localhost:11434/v1", model: "llama3.2" },
  ];

  const applyPreset = (preset: (typeof llmPresets)[0]) => {
    setLlmLabel(preset.label);
    setLlmUrl(preset.url);
    setLlmModel(preset.model);
  };

  // คำเตือนเมื่อใช้ http:// กับ host ที่ไม่ใช่ localhost/Tailscale (100.64–100.127.x.x)
  const recorderHttpWarning = (url: string): string | null => {
    const trimmed = url.trim();
    if (!/^http:\/\//i.test(trimmed)) return null;
    const rest = trimmed.replace(/^http:\/\//i, "");
    const host = (
      rest.startsWith("[") ? rest.slice(1, rest.indexOf("]")) : rest.split(/[/:?]/)[0] ?? ""
    ).toLowerCase();
    const parts = host.split(".");
    const isIpv4 = parts.length === 4 && parts.every((p) => /^\d{1,3}$/.test(p) && Number(p) <= 255);
    const isPrivate =
      host === "localhost" ||
      host === "::1" ||
      (isIpv4 &&
        (Number(parts[0]) === 127 ||
          (Number(parts[0]) === 100 && Number(parts[1]) >= 64 && Number(parts[1]) <= 127)));
    return isPrivate
      ? null
      : "ใช้ http เปล่าๆ ข้อมูลจะไม่ถูกเข้ารหัสระหว่างทาง — ปลอดภัยเฉพาะ localhost หรือ Tailscale แนะนำใช้ https";
  };

  const handleTest = async () => {
    if (!webullKey.trim() || (!webullSecret.trim() && !hasSecret)) {
      setError("กรุณาใส่ Webull App Key และ Secret ก่อนทดสอบ");
      return;
    }
    setTesting(true);
    setError("");
    setTestResult(null);
    setDetectedScheme(null);
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const r = await invoke<ConnectionTestResult>("test_connection", {
        webullKey: webullKey.trim(),
        webullSecret: webullSecret.trim() || null,
        webullAccessToken: webullToken.trim() || null,
        webullBaseUrl: webullUrl.trim() || null,
      });
      setTestResult(r);
      setDetectedScheme(r.signature_scheme ?? null);
    } catch (e) {
      setError(`ทดสอบไม่สำเร็จ: ${e}`);
    } finally {
      setTesting(false);
    }
  };

  const handleCreateToken = async () => {
    setCreatingToken(true);
    setError("");
    setTokenNotice(null);
    setTokenIssued(false);
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      // the token value stays native — only token_issued crosses IPC; the
      // pending token is picked up by the next save
      const r = await invoke<{
        ok: boolean;
        status: string;
        token_issued: boolean;
        expires_ms?: number | null;
        detail: string;
      }>("create_access_token", {
        webullKey: webullKey.trim() || null,
        webullSecret: webullSecret.trim() || null,
        webullBaseUrl: webullUrl.trim() || null,
      });
      setTokenIssued(r.ok && r.token_issued);
      setTokenNotice({ ok: r.ok, text: r.detail });
    } catch (e) {
      setTokenNotice({ ok: false, text: `สร้าง token ไม่สำเร็จ: ${e}` });
    } finally {
      setCreatingToken(false);
    }
  };

  const handleLlmTest = async () => {
    if (!llmUrl.trim() || !llmModel.trim() || (!llmKey.trim() && !hasLlmKey)) {
      setError("กรุณาใส่ Base URL / Model / API Key ของ AI ก่อนทดสอบ");
      return;
    }
    setTestingLlm(true);
    setError("");
    setLlmResult(null);
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const r = await invoke<ConnectionTestResult>("test_llm", {
        llmLabel: llmLabel.trim() || null,
        llmBaseUrl: llmUrl.trim(),
        llmModel: llmModel.trim(),
        llmApiKey: llmKey.trim() || null,
      });
      setLlmResult(r);
    } catch (e) {
      setError(`ทดสอบ AI ไม่สำเร็จ: ${e}`);
    } finally {
      setTestingLlm(false);
    }
  };

  const handleRecorderTest = async () => {
    if (!recorderUrl.trim()) {
      setError("กรุณาใส่ที่อยู่ server ก่อนทดสอบ");
      return;
    }
    setTestingRecorder(true);
    setError("");
    setRecorderResult(null);
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const r = await invoke<RecorderTestResult>("recorder_test_connection", {
        url: recorderUrl.trim(),
        token: recorderToken.trim() || null,
      });
      setRecorderResult(r);
    } catch (e) {
      setError(`ทดสอบไม่สำเร็จ: ${e}`);
    } finally {
      setTestingRecorder(false);
    }
  };

  const handleSave = async () => {
    if (!webullKey.trim() || (!webullSecret.trim() && !hasSecret)) {
      setError("กรุณาใส่ Webull App Key และ Secret");
      return;
    }
    if (labSource === "api") {
      if (!recorderUrl.trim()) {
        setError("กรุณาใส่ที่อยู่ server ของ recorder");
        return;
      }
      if (!recorderToken.trim() && !hasRecorderToken) {
        setError("กรุณาใส่ API token ของ server");
        return;
      }
    }
    setSaving(true);
    setError("");
    try {
      // In Tauri mode: invoke save_credentials
      // In dev mode: just complete (env vars assumed)
      if (hasTauri) {
        const { invoke } = await import("@tauri-apps/api/core");
        await invoke("save_credentials", {
          webullKey: webullKey.trim(),
          webullSecret: webullSecret.trim(),
          webullBaseUrl: webullUrl.trim() || null,
          webullAccessToken: webullToken.trim() || null,
          webullSignatureScheme: detectedScheme,
          llmLabel: llmLabel.trim() || null,
          llmBaseUrl: llmUrl.trim() || null,
          llmModel: llmModel.trim() || null,
          llmApiKey: llmKey.trim() || null,
        });
        // แหล่งข้อมูล Lab — ช่อง token ที่เว้นว่าง = ใช้ค่าที่บันทึกไว้
        // (สลับกลับเป็น local ไม่ลบ URL/token ที่เก็บไว้)
        await invoke("save_lab_source", {
          labSource,
          recorderApiUrl: recorderUrl.trim(),
          recorderApiToken: recorderToken.trim() || null,
        });
      }
      onComplete();
    } catch (e) {
      setError(`บันทึกไม่สำเร็จ: ${e}`);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        background: "var(--canvas)",
        zIndex: 999,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        fontFamily: "'Inter', 'IBM Plex Sans Thai', 'Segoe UI', sans-serif",
        color: "var(--text)",
      }}
    >
      <div
        style={{
          width: 540,
          maxHeight: "92vh",
          overflowY: "auto",
          background: "var(--surface2)",
          border: "1px solid var(--border)",
          borderRadius: 10,
          padding: 32,
        }}
      >
        <h1
          style={{
            fontSize: 20,
            fontWeight: 750,
            marginBottom: 4,
            letterSpacing: "-0.02em",
          }}
        >
          PREMIUM RADAR
        </h1>
        <p style={{ color: "var(--dim)", fontSize: 12, marginBottom: 24 }}>
          {isEdit
            ? "ตั้งค่า API keys — โหลดค่าที่บันทึกไว้มาให้แก้ไขแล้ว"
            : "ตั้งค่าครั้งแรก — ใส่ API keys เพื่อเริ่มใช้งาน"}
        </p>

        {/* Webull section */}
        <div style={{ marginBottom: 24 }}>
          <h2
            style={{
              fontSize: 11,
              fontWeight: 700,
              color: "var(--secondary)",
              letterSpacing: "0.08em",
              textTransform: "uppercase",
              marginBottom: 10,
            }}
          >
            Webull API (จำเป็น)
          </h2>
          <input
            placeholder="App Key"
            value={webullKey}
            onChange={(e) => setWebullKey(e.target.value)}
            style={inputStyle}
            type="password"
            autoComplete="off"
          />
          <input
            placeholder={hasSecret ? `App Secret (${keptHint})` : "App Secret"}
            value={webullSecret}
            onChange={(e) => setWebullSecret(e.target.value)}
            style={inputStyle}
            type="password"
            autoComplete="off"
          />
          <input
            placeholder={
              tokenIssued
                ? "token ออกแล้ว (รอยืนยัน SMS) — กด บันทึก เพื่อใช้ token นี้"
                : hasToken
                  ? `Access Token (${keptHint})`
                  : "Access Token (จำเป็นบน production — สร้างด้วยปุ่มด้านล่างแล้วยืนยัน SMS ในแอป Webull)"
            }
            value={webullToken}
            onChange={(e) => setWebullToken(e.target.value)}
            style={inputStyle}
            type="password"
            autoComplete="off"
          />
          <input
            placeholder="Base URL (เว้นว่าง = https://api.webull.co.th)"
            value={webullUrl}
            onChange={(e) => setWebullUrl(e.target.value)}
            style={inputStyle}
            autoComplete="off"
          />
          <div style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
            <button
              onClick={handleCreateToken}
              disabled={creatingToken || !hasTauri}
              style={{
                background: "var(--surface2)",
                color: creatingToken ? "var(--dim)" : "var(--th-label)",
                border: "1px solid var(--border-strong)",
                borderRadius: 6,
                padding: "6px 12px",
                fontSize: 11,
                fontWeight: 650,
                cursor: creatingToken || !hasTauri ? "not-allowed" : "pointer",
                whiteSpace: "nowrap",
              }}
            >
              {creatingToken ? "กำลังสร้าง..." : "สร้าง Access Token ใหม่ (SMS)"}
            </button>
            {tokenIssued && (
              <span
                style={{
                  fontSize: 11,
                  lineHeight: 1.5,
                  color: "var(--amber)",
                  whiteSpace: "nowrap",
                }}
              >
                ⏳ token ออกแล้ว (รอยืนยัน SMS)
              </span>
            )}
            {tokenNotice && (
              <span
                style={{
                  fontSize: 11,
                  lineHeight: 1.5,
                  color: tokenNotice.ok ? "var(--green)" : "var(--red)",
                }}
              >
                {tokenNotice.ok ? "✓ " : "✕ "}
                {tokenNotice.text}
              </span>
            )}
          </div>
          <div style={{ display: "flex", gap: 8, alignItems: "center", marginTop: 4 }}>
            <button
              onClick={handleTest}
              disabled={testing || !hasTauri}
              style={{
                background: "var(--surface2)",
                color: testing ? "var(--dim)" : "var(--th-label)",
                border: "1px solid var(--border-strong)",
                borderRadius: 6,
                padding: "7px 14px",
                fontSize: 11,
                fontWeight: 650,
                cursor: testing || !hasTauri ? "not-allowed" : "pointer",
                whiteSpace: "nowrap",
              }}
            >
              {testing ? "กำลังทดสอบ..." : "ทดสอบการเชื่อมต่อ"}
            </button>
            {testResult && (
              <span
                style={{
                  fontSize: 11,
                  lineHeight: 1.5,
                  color: testResult.webull_ok ? "var(--green)" : "var(--red)",
                }}
              >
                {testResult.webull_ok ? "✓ " : "✕ "}
                {testResult.detail}
                {testResult.streaming === "entitled" && (
                  <span style={{ color: "var(--green)" }}> · 📡 ราคาสด: มีสิทธิ์ streaming</span>
                )}
                {testResult.streaming === "limit" && (
                  <span style={{ color: "var(--amber)" }}>
                    {" "}
                    · 📡 ราคาสด: มีสิทธิ์ (ครบโควตา 5 จุดชั่วคราว — ลองใหม่ใน ~1 นาที)
                  </span>
                )}
                {testResult.streaming === "none" && (
                  <span style={{ color: "var(--amber)" }}>
                    {" "}
                    · 📡 ราคาสด: AppKey นี้ไม่มีสิทธิ์ streaming (สแกนยังใช้ REST ได้ปกติ)
                  </span>
                )}
                {testResult.streaming === "unknown" && (
                  <span style={{ color: "var(--secondary)" }}> · 📡 ราคาสด: ตรวจไม่ได้ (timeout/เครือข่าย)</span>
                )}
              </span>
            )}
          </div>
        </div>

        {/* LLM section */}
        <div style={{ marginBottom: 24 }}>
          <h2
            style={{
              fontSize: 11,
              fontWeight: 700,
              color: "var(--secondary)",
              letterSpacing: "0.08em",
              textTransform: "uppercase",
              marginBottom: 10,
            }}
          >
            AI Assistant (ไม่บังคับ)
          </h2>
          <div style={{ display: "flex", gap: 6, marginBottom: 10 }}>
            {llmPresets.map((p) => (
              <button
                key={p.label}
                onClick={() => applyPreset(p)}
                style={{
                  ...presetBtnStyle,
                  background:
                    llmUrl === p.url ? "var(--blue-soft)" : "var(--surface2)",
                  color: llmUrl === p.url ? "var(--text)" : "var(--secondary)",
                }}
              >
                {p.label}
              </button>
            ))}
          </div>
          <input
            placeholder="Label (เช่น Z.ai GLM-5.3)"
            value={llmLabel}
            onChange={(e) => setLlmLabel(e.target.value)}
            style={inputStyle}
            autoComplete="off"
          />
          <input
            placeholder="Base URL (เช่น https://api.z.ai/api/paas/v4)"
            value={llmUrl}
            onChange={(e) => setLlmUrl(e.target.value)}
            style={inputStyle}
            autoComplete="off"
          />
          <input
            placeholder="Model (เช่น glm-5.3)"
            value={llmModel}
            onChange={(e) => setLlmModel(e.target.value)}
            style={inputStyle}
            autoComplete="off"
          />
          <input
            placeholder={hasLlmKey ? `API Key (${keptHint})` : "API Key"}
            value={llmKey}
            onChange={(e) => setLlmKey(e.target.value)}
            style={inputStyle}
            type="password"
            autoComplete="off"
          />
          <div style={{ display: "flex", gap: 8, alignItems: "center", marginTop: 4 }}>
            <button
              onClick={handleLlmTest}
              disabled={testingLlm || !hasTauri}
              style={{
                background: "var(--surface2)",
                color: testingLlm ? "var(--dim)" : "var(--th-label)",
                border: "1px solid var(--border-strong)",
                borderRadius: 6,
                padding: "7px 14px",
                fontSize: 11,
                fontWeight: 650,
                cursor: testingLlm || !hasTauri ? "not-allowed" : "pointer",
                whiteSpace: "nowrap",
              }}
            >
              {testingLlm ? "กำลังทดสอบ AI..." : "ทดสอบ AI"}
            </button>
            {llmResult && (
              <span
                style={{
                  fontSize: 11,
                  lineHeight: 1.5,
                  color: llmResult.webull_ok ? "var(--green)" : "var(--red)",
                }}
              >
                {llmResult.webull_ok ? "✓ " : "✕ "}
                {llmResult.detail}
              </span>
            )}
          </div>
        </div>

        {/* Lab data source section */}
        <div style={{ marginBottom: 24 }}>
          <h2
            style={{
              fontSize: 11,
              fontWeight: 700,
              color: "var(--secondary)",
              letterSpacing: "0.08em",
              textTransform: "uppercase",
              marginBottom: 10,
            }}
          >
            แหล่งข้อมูล Lab
          </h2>
          <div style={{ display: "flex", gap: 6, marginBottom: 10 }}>
            <button
              onClick={() => setLabSource("local")}
              style={{
                ...choiceBtnStyle,
                background: labSource === "local" ? "var(--blue-soft)" : "var(--surface2)",
                color: labSource === "local" ? "var(--text)" : "var(--secondary)",
              }}
            >
              <b>ในเครื่องนี้ (ค่าเริ่มต้น)</b>
              <span style={{ fontWeight: 400, fontSize: 10 }}>อ่านข้อมูลที่แอปบันทึกเองในเครื่องนี้</span>
            </button>
            <button
              onClick={() => setLabSource("api")}
              style={{
                ...choiceBtnStyle,
                background: labSource === "api" ? "var(--blue-soft)" : "var(--surface2)",
                color: labSource === "api" ? "var(--text)" : "var(--secondary)",
              }}
            >
              <b>Recorder server</b>
              <span style={{ fontWeight: 400, fontSize: 10 }}>เก็บข้อมูลให้แม้ปิดแอป</span>
            </button>
          </div>
          {labSource === "api" && (
            <>
              <input
                placeholder="ที่อยู่ server"
                value={recorderUrl}
                onChange={(e) => {
                  setRecorderUrl(e.target.value);
                  setRecorderResult(null);
                }}
                style={inputStyle}
                autoComplete="off"
              />
              {recorderHttpWarning(recorderUrl) && (
                <p style={{ color: "var(--amber)", fontSize: 10, lineHeight: 1.5, margin: "0 0 8px" }}>
                  ⚠ {recorderHttpWarning(recorderUrl)}
                </p>
              )}
              <input
                placeholder={hasRecorderToken ? `API token (${keptHint})` : "API token"}
                value={recorderToken}
                onChange={(e) => setRecorderToken(e.target.value)}
                style={inputStyle}
                type="password"
                autoComplete="off"
              />
              <div style={{ display: "flex", gap: 8, alignItems: "center", marginTop: 4 }}>
                <button
                  onClick={handleRecorderTest}
                  disabled={testingRecorder || !hasTauri}
                  style={{
                    background: "var(--surface2)",
                    color: testingRecorder ? "var(--dim)" : "var(--th-label)",
                    border: "1px solid var(--border-strong)",
                    borderRadius: 6,
                    padding: "7px 14px",
                    fontSize: 11,
                    fontWeight: 650,
                    cursor: testingRecorder || !hasTauri ? "not-allowed" : "pointer",
                    whiteSpace: "nowrap",
                  }}
                >
                  {testingRecorder ? "กำลังทดสอบ..." : "ทดสอบการเชื่อมต่อ"}
                </button>
                {recorderResult && (
                  <span
                    style={{
                      fontSize: 11,
                      lineHeight: 1.5,
                      color: recorderResult.ok ? "var(--green)" : "var(--red)",
                    }}
                  >
                    {recorderResult.ok ? "✓ " : "✕ "}
                    {recorderResult.message_th}
                  </span>
                )}
              </div>
            </>
          )}
        </div>

        {error && (
          <p style={{ color: "var(--red)", fontSize: 11, marginBottom: 12 }}>{error}</p>
        )}

        <div style={{ display: "flex", gap: 8 }}>
          <button
            onClick={handleSave}
            disabled={saving}
            style={{
              flex: 1,
              background: saving ? "var(--blue-soft)" : "#2b6cb8",
              color: "var(--text)",
              border: "1px solid var(--blue)",
              borderRadius: 7,
              padding: "10px 0",
              fontSize: 13,
              fontWeight: 650,
              cursor: saving ? "not-allowed" : "pointer",
            }}
          >
            {saving ? "กำลังบันทึก..." : isEdit ? "บันทึกการแก้ไข" : "บันทึกและเริ่มใช้งาน"}
          </button>
          {onSkip && (
            <button
              onClick={onSkip}
              style={{
                background: "transparent",
                color: "var(--dim)",
                border: "1px solid var(--border-strong)",
                borderRadius: 7,
                padding: "10px 16px",
                fontSize: 12,
                cursor: "pointer",
                whiteSpace: "nowrap",
              }}
            >
              {isEdit ? "ยกเลิก" : "ข้าม (ใช้ env vars)"}
            </button>
          )}
        </div>

        {!isEdit && (
          <p style={{ color: "var(--dim)", fontSize: 10, lineHeight: 1.7, marginTop: 14 }}>
            "ข้าม" = ใช้ตัวแปร environment ของโปรเซสแอป แทนไฟล์ settings —
            อ่าน <code style={{ color: "var(--secondary)" }}>WEBULL_APP_KEY</code>,{" "}
            <code style={{ color: "var(--secondary)" }}>WEBULL_APP_SECRET</code>,{" "}
            <code style={{ color: "var(--secondary)" }}>WEBULL_ACCESS_TOKEN</code> (ถ้ามี) และ
            <code style={{ color: "var(--secondary)" }}> LLM_BASE_URL + LLM_API_KEY + LLM_MODEL</code>{" "}
            (หรือ <code style={{ color: "var(--secondary)" }}>GLM_API_KEY</code> /{" "}
            <code style={{ color: "var(--secondary)" }}>OPENAI_API_KEY</code>) —
            ใช้ได้เฉพาะตอนเปิดแอปจาก terminal ที่ set ตัวแปรไว้ ถ้าดับเบิลคลิกแอปปกติจะไม่เห็นค่าเหล่านี้
          </p>
        )}
      </div>
    </div>
  );
}

const inputStyle: React.CSSProperties = {
  width: "100%",
  background: "var(--surface)",
  border: "1px solid var(--border-strong)",
  borderRadius: 6,
  color: "var(--text)",
  padding: "8px 12px",
  fontSize: 12,
  marginBottom: 8,
  outline: "none",
  boxSizing: "border-box",
};

const presetBtnStyle: React.CSSProperties = {
  flex: 1,
  border: "1px solid var(--border-strong)",
  borderRadius: 5,
  padding: "6px 4px",
  fontSize: 10,
  fontWeight: 600,
  cursor: "pointer",
};

const choiceBtnStyle: React.CSSProperties = {
  flex: 1,
  display: "flex",
  flexDirection: "column",
  gap: 3,
  alignItems: "flex-start",
  textAlign: "left",
  border: "1px solid var(--border-strong)",
  borderRadius: 5,
  padding: "8px 10px",
  fontSize: 11,
  cursor: "pointer",
};
