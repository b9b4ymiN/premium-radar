import type { DrawingTool, Tool } from "./drawingsStore";

const DRAWING_TOOLS = {
  segment: { title: "เส้นแนวโน้ม", icon: "M3 16 16 3 M3 14v4 M16 1v4" },
  rayLine: { title: "รังสี", icon: "M3 16 17 2 M12 2h5v5 M3 14v4" },
  horizontalStraightLine: { title: "เส้นแนวนอน", icon: "M2 10h16 M10 7v6" },
  verticalStraightLine: { title: "เส้นแนวตั้ง", icon: "M10 2v16 M7 10h6" },
  parallelStraightLine: { title: "ช่องแนวโน้มคู่ขนาน", icon: "M2 12 12 2 M3 18 18 3" },
  fibonacciLine: { title: "ฟีโบนักชีรีเทรซเมนต์", icon: "M2 3h16 M2 7h12 M2 11h9 M2 15h6 M2 18h4" },
  simpleAnnotation: { title: "ข้อความกำกับ", icon: "M4 5V3h12v2 M10 3v14 M7 17h6" },
} satisfies Record<DrawingTool, { title: string; icon: string }>;

export const drawingTitle = (tool: DrawingTool) => DRAWING_TOOLS[tool].title;

interface Props {
  tool: Tool;
  locked: boolean;
  magnet: boolean;
  canDelete: boolean;
  hasDrawings: boolean;
  onTool: (tool: Tool) => void;
  onLock: () => void;
  onMagnet: () => void;
  onDelete: () => void;
  onClear: () => void;
}

interface ButtonProps {
  title: string;
  icon: string;
  action: () => void;
  pressed?: boolean;
  disabled?: boolean;
}

function ToolButton({ title, icon, action, pressed, disabled }: ButtonProps) {
  return (
    <button type="button" className="pc-tool" title={title} aria-label={title}
      aria-pressed={pressed} disabled={disabled} onClick={action}>
      <svg viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.35"
        strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
        <path d={icon} />
      </svg>
    </button>
  );
}

export function DrawingToolbar({
  tool, locked, magnet, canDelete, hasDrawings, onTool, onLock, onMagnet, onDelete, onClear,
}: Props) {
  return (
    <div className="pc-tools" role="group" aria-label="เครื่องมือวาดกราฟ">
      <ToolButton title="เคอร์เซอร์" icon="M5 2v14l4-4 3 6 2-1-3-6h6Z"
        pressed={tool === "cursor"} action={() => onTool("cursor")} />
      {(Object.keys(DRAWING_TOOLS) as DrawingTool[]).map(name => (
        <ToolButton key={name} title={DRAWING_TOOLS[name].title} icon={DRAWING_TOOLS[name].icon}
          pressed={tool === name} disabled={locked} action={() => onTool(name)} />
      ))}
      <div className="pc-tool-divider" />
      <ToolButton title="ล็อกเส้นและข้อความ"
        icon={locked ? "M5 8h10v9H5Z M7 8V5a3 3 0 0 1 6 0v3 M10 11v3" : "M5 8h10v9H5Z M7 8V5a3 3 0 0 1 6 0 M10 11v3"}
        pressed={locked} action={onLock} />
      <ToolButton title="แม่เหล็กจับราคาเปิด สูง ต่ำ ปิด"
        icon="M3 3h4v8a3 3 0 0 0 6 0V3h4v8a7 7 0 0 1-14 0Z M3 7h4 M13 7h4"
        pressed={magnet} action={onMagnet} />
      <ToolButton title="ลบรายการที่เลือก" icon="M4 5h12 M7 5V3h6v2 M6 5l1 12h6l1-12 M9 8v6 M11 8v6"
        disabled={!canDelete} action={onDelete} />
      <ToolButton title="ลบเส้นและข้อความทั้งหมด" icon="M5 5l10 10 M15 5 5 15 M2 2h16v16H2Z"
        disabled={!hasDrawings || locked} action={onClear} />
    </div>
  );
}
