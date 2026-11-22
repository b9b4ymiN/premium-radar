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
}

interface StoredLlm {
  label?: string;
  base_url?: string;
  model?: string;
  api_key?: string;
}

interface StoredSettings {
  webull_app_key?: string;
  webull_app_secret?: string;
  webull_access_token?: string;
  webull_base_url?: string;
  llm?: StoredLlm | null;
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
  const [testingLlm, setTestingLlm] = useState(false);
  const [llmResult, setLlmResult] = useState<ConnectionTestResult | null>(null);
  const [error, setError] = useState("");

  const isEdit = mode === "edit";

  // Load SAVED settings into the fields so reopening ⚙ shows what's stored
  // (previously every field started blank, which looked like everything was wiped).
  // The Base URL prefill shows the EFFECTIVE default explicitly — an empty
  // looking field made users think the URL was still configured when the app
  // was actually falling back to production.
  useEffect(() => {
    if (!hasTauri) return;
    import("@tauri-apps/api/core")
      .then(({ invoke }) => invoke<StoredSettings>("get_settings"))
      .then((st) => {
        setWebullKey(st?.webull_app_key ?? "");
        setWebullSecret(st?.webull_app_secret ?? "");
        setWebullToken(st?.webull_access_token ?? "");
        setWebullUrl(st?.webull_base_url?.trim() || "https://api.webull.co.th");
        setLlmLabel(st?.llm?.label ?? "");
        setLlmUrl(st?.llm?.base_url ?? "");
        setLlmModel(st?.llm?.model ?? "");
        setLlmKey(st?.llm?.api_key ?? "");
      })
      .catch(() => {});
  }, []);

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

  const handleTest = async () => {
    if (!webullKey.trim() || !webullSecret.trim()) {
      setError("กรุณาใส่ Webull App Key และ Secret ก่อนทดสอบ");
      return;
    }
    setTesting(true);
    setError("");
    setTestResult(null);
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const r = await invoke<ConnectionTestResult>("test_connection", {
        webullKey: webullKey.trim(),
        webullSecret: webullSecret.trim(),
        webullAccessToken: webullToken.trim() || null,
        webullBaseUrl: webullUrl.trim() || null,
      });
      setTestResult(r);
    } catch (e) {
      setError(`ทดสอบไม่สำเร็จ: ${e}`);
    } finally {
      setTesting(false);
    }
  };

  const handleLlmTest = async () => {
    if (!llmUrl.trim() || !llmModel.trim() || !llmKey.trim()) {
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
        llmApiKey: llmKey.trim(),
      });
      setLlmResult(r);
    } catch (e) {
      setError(`ทดสอบ AI ไม่สำเร็จ: ${e}`);
    } finally {
      setTestingLlm(false);
    }
  };

  const handleSave = async () => {
    if (!webullKey.trim() || !webullSecret.trim()) {
      setError("กรุณาใส่ Webull App Key และ Secret");
      return;
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
          llmLabel: llmLabel.trim() || null,
          llmBaseUrl: llmUrl.trim() || null,
          llmModel: llmModel.trim() || null,
          llmApiKey: llmKey.trim() || null,
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
        background: "var(--canvas, #0b0d11)",
        zIndex: 999,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        fontFamily: "'Inter', 'IBM Plex Sans Thai', 'Segoe UI', sans-serif",
        color: "#f0f3f8",
      }}
    >
      <div
        style={{
          width: 540,
          maxHeight: "92vh",
          overflowY: "auto",
          background: "#151a21",
          border: "1px solid #292f39",
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
        <p style={{ color: "#778394", fontSize: 12, marginBottom: 24 }}>
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
              color: "#a2adbc",
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
            placeholder="App Secret"
            value={webullSecret}
            onChange={(e) => setWebullSecret(e.target.value)}
            style={inputStyle}
            type="password"
            autoComplete="off"
          />
          <input
            placeholder="Access Token (จำเป็นบน production — เอาจาก vcpSystem .env: WEBULL_ACCESS_TOKEN)"
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
          <div style={{ display: "flex", gap: 8, alignItems: "center", marginTop: 4 }}>
            <button
              onClick={handleTest}
              disabled={testing || !hasTauri}
              style={{
                background: "#1a2029",
                color: testing ? "#6b7d98" : "#cfd9e6",
                border: "1px solid #384251",
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
                  color: testResult.webull_ok ? "#3ddc97" : "#f4788c",
                }}
              >
                {testResult.webull_ok ? "✓ " : "✕ "}
                {testResult.detail}
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
              color: "#a2adbc",
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
                    llmUrl === p.url ? "#1b365a" : "#1a2029",
                  color: llmUrl === p.url ? "#f2f7ff" : "#aab7c8",
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
            placeholder="API Key"
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
                background: "#1a2029",
                color: testingLlm ? "#6b7d98" : "#cfd9e6",
                border: "1px solid #384251",
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
                  color: llmResult.webull_ok ? "#3ddc97" : "#f4788c",
                }}
              >
                {llmResult.webull_ok ? "✓ " : "✕ "}
                {llmResult.detail}
              </span>
            )}
          </div>
        </div>

        {error && (
          <p style={{ color: "#f4788c", fontSize: 11, marginBottom: 12 }}>{error}</p>
        )}

        <div style={{ display: "flex", gap: 8 }}>
          <button
            onClick={handleSave}
            disabled={saving}
            style={{
              flex: 1,
              background: saving ? "#1a365a" : "#2b6cb8",
              color: "#fff",
              border: "1px solid #4a91fa",
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
                color: "#778394",
                border: "1px solid #384251",
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
          <p style={{ color: "#5f6b7c", fontSize: 10, lineHeight: 1.7, marginTop: 14 }}>
            "ข้าม" = ใช้ตัวแปร environment ของโปรเซสแอป แทนไฟล์ settings —
            อ่าน <code style={{ color: "#8da7ca" }}>WEBULL_APP_KEY</code>,{" "}
            <code style={{ color: "#8da7ca" }}>WEBULL_APP_SECRET</code>,{" "}
            <code style={{ color: "#8da7ca" }}>WEBULL_ACCESS_TOKEN</code> (ถ้ามี) และ
            <code style={{ color: "#8da7ca" }}> LLM_BASE_URL + LLM_API_KEY + LLM_MODEL</code>{" "}
            (หรือ <code style={{ color: "#8da7ca" }}>GLM_API_KEY</code> /{" "}
            <code style={{ color: "#8da7ca" }}>OPENAI_API_KEY</code>) —
            ใช้ได้เฉพาะตอนเปิดแอปจาก terminal ที่ set ตัวแปรไว้ ถ้าดับเบิลคลิกแอปปกติจะไม่เห็นค่าเหล่านี้
          </p>
        )}
      </div>
    </div>
  );
}

const inputStyle: React.CSSProperties = {
  width: "100%",
  background: "#0e1319",
  border: "1px solid #384251",
  borderRadius: 6,
  color: "#f0f3f8",
  padding: "8px 12px",
  fontSize: 12,
  marginBottom: 8,
  outline: "none",
  boxSizing: "border-box",
};

const presetBtnStyle: React.CSSProperties = {
  flex: 1,
  border: "1px solid #353d49",
  borderRadius: 5,
  padding: "6px 4px",
  fontSize: 10,
  fontWeight: 600,
  cursor: "pointer",
};
