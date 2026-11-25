/** Step 5 — the Webull field map and the "not yet" screen, ported from the
 *  prototype. No order is ever placed: the user types the values into Webull
 *  by hand. */

import type { EasyIdea, EasyIdeas } from "../types/easy";
import { EzIcon } from "./icons";
import { expiryTh, usd } from "./util";

export function WebullFieldMap({ idea: o, ideas: d, onBack, onDone }: {
  idea: EasyIdea;
  ideas: EasyIdeas;
  onBack: () => void;
  onDone: () => void;
}) {
  const rows: Array<[string, string, string?]> = [
    ["หุ้น", d.symbol],
    ["ประเภท", "Put", "สัญญาที่ “ยอมซื้อหุ้น”"],
    ["การกระทำ", "Sell to Open (ขายเปิด)", "คุณเป็นคนขายและรับเงิน"],
    ["วันหมดอายุ", o.expiry, expiryTh(o.expiry)],
    ["ราคาที่ยอมซื้อ (Strike)", `$${o.strike}`],
    ["จำนวน", "1 สัญญา", "= 100 หุ้น"],
    ["ประเภทคำสั่ง", "Limit", "กำหนดราคาเอง ไม่ใช้ Market"],
    ["ราคา", `$${o.premium.toFixed(2)} หรือสูงกว่า`, `ได้เงิน ${usd(o.credit)} ต่อสัญญา`],
    ["อายุคำสั่ง", "Day", "ถ้าไม่มีคนซื้อในวันนั้น คำสั่งจะยกเลิกเอง"],
  ];
  return (
    <section className="ez-screen">
      <button className="ez-back" onClick={onBack}><EzIcon name="back" />ย้อนกลับ</button>
      <h1>กรอกแบบนี้ในแอป Webull</h1>
      <p className="ez-lead">ตอนนี้ PREMIUM RADAR ยังไม่ส่งคำสั่งให้ คุณกดเองในแอป Webull โดยใช้ค่าด้านล่าง</p>
      <div className="ez-panel ez-gap-l">
        <table className="ez-fieldmap">
          <tbody>
            {rows.map(([k, v, n]) => (
              <tr key={k}><td>{k}</td><td>{v}{n && <small>{n}</small>}</td></tr>
            ))}
          </tbody>
        </table>
      </div>
      <p className="ez-note warn" style={{ marginTop: 16 }}>
        <EzIcon name="alert" />
        <span>ชื่อช่องในแอป Webull อาจต่างไปเล็กน้อยตามเวอร์ชัน ก่อนกดยืนยัน ให้ดูหน้าสรุปของ Webull ว่าเขียน “Sell to Open” และ Put</span>
      </p>
      <div className="ez-actions">
        <button className="ez-btn primary" onClick={onDone}>กดใน Webull แล้ว กลับหน้าแรก</button>
        <button className="ez-btn ghost" onClick={onDone}>กลับหน้าแรก</button>
      </div>
    </section>
  );
}

export function NotYet({ onHome }: { onHome: () => void }) {
  return (
    <section className="ez-screen">
      <div className="ez-agent">
        <span className="ez-avatar"><EzIcon name="radar" /></span>
        <div>
          <div className="ez-agent-name">ผู้ช่วย</div>
          <p className="ez-say">ไม่เป็นไรเลยครับ การไม่ทำก็เป็นการตัดสินใจที่ดีได้ ถ้าคืนพรุ่งนี้มีไอเดียที่ตรงกับคุณมากกว่านี้ ผมจะบอกในหน้าแรก</p>
        </div>
      </div>
      <div className="ez-actions"><button className="ez-btn primary" onClick={onHome}>กลับหน้าแรก</button></div>
    </section>
  );
}
