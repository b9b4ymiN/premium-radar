/**
 * First-run setup screen — lets users configure Webull + LLM keys.
 * Shows when `needs_setup()` returns true (no Webull keys configured).
 * Accessible from settings menu anytime.
 */

import { useState } from "react";

interface SetupScreenProps {
  onComplete: () => void;
  onSkip?: () => void; // allow "use env vars" for dev mode
}

export function SetupScreen({ onComplete, onSkip }: SetupScreenProps) {
  const [webullKey, setWebullKey] = useState("");
  const [webullSecret, setWebullSecret] = useState("");
  const [webullUrl, setWebullUrl] = useState("");
  const [llmLabel, setLlmLabel] = useState("");
  const [llmUrl, setLlmUrl] = useState("");
  const [llmModel, setLlmModel] = useState("");
  const [llmKey, setLlmKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  const llmPresets = [
    { label: "Z.ai GLM-5.3", url: "https://api.z.ai", model: "glm-5.3" },
    { label: "OpenAI GPT-4o", url: "https://api.openai.com", model: "gpt-4o" },
    { label: "Ollama (local)", url: "http://localhost:11434", model: "llama3.2" },
  ];

  const applyPreset = (preset: (typeof llmPresets)[0]) => {
    setLlmLabel(preset.label);
    setLlmUrl(preset.url);
    setLlmModel(preset.model);
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
      const hasTauri =
        typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
      if (hasTauri) {
        const { invoke } = await import("@tauri-apps/api/core");
        await invoke("save_credentials", {
          webullKey: webullKey.trim(),
          webullSecret: webullSecret.trim(),
          webullBaseUrl: webullUrl.trim() || null,
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
          width: 520,
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
          ตั้งค่าครั้งแรก — ใส่ API keys เพื่อเริ่มใช้งาน
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
          />
          <input
            placeholder="App Secret"
            value={webullSecret}
            onChange={(e) => setWebullSecret(e.target.value)}
            style={inputStyle}
            type="password"
          />
          <input
            placeholder="Base URL (เว้นว่าง = https://api.webull.co.th)"
            value={webullUrl}
            onChange={(e) => setWebullUrl(e.target.value)}
            style={inputStyle}
          />
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
          />
          <input
            placeholder="Base URL (เช่น https://api.z.ai)"
            value={llmUrl}
            onChange={(e) => setLlmUrl(e.target.value)}
            style={inputStyle}
          />
          <input
            placeholder="Model (เช่น glm-5.3)"
            value={llmModel}
            onChange={(e) => setLlmModel(e.target.value)}
            style={inputStyle}
          />
          <input
            placeholder="API Key"
            value={llmKey}
            onChange={(e) => setLlmKey(e.target.value)}
            style={inputStyle}
            type="password"
          />
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
            {saving ? "กำลังบันทึก..." : "บันทึกและเริ่มใช้งาน"}
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
              }}
            >
              ข้าม (ใช้ env vars)
            </button>
          )}
        </div>
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
