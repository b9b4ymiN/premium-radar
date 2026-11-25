/** Easy Mode inline SVG sprite — ported verbatim from the approved prototype
 *  (`design/easy-mode-prototype.html`). Rendered once by EasyApp; icons are
 *  referenced through <EzIcon name="…" /> so no emoji/glyph icons appear. */

export function EzSprite() {
  return (
    <svg width="0" height="0" style={{ position: "absolute" }} aria-hidden="true">
      <symbol id="ez-i-radar" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9" /><circle cx="12" cy="12" r="5" /><path d="M12 12l6-6" /></symbol>
      <symbol id="ez-i-chev" viewBox="0 0 24 24"><path d="M9 6l6 6-6 6" /></symbol>
      <symbol id="ez-i-back" viewBox="0 0 24 24"><path d="M15 6l-6 6 6 6" /></symbol>
      <symbol id="ez-i-check" viewBox="0 0 24 24"><path d="M5 12.5l4.5 4.5L19 7.5" /></symbol>
      <symbol id="ez-i-alert" viewBox="0 0 24 24"><path d="M12 4l9 16H3z" /><path d="M12 10v4M12 17.2v.1" /></symbol>
      <symbol id="ez-i-info" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9" /><path d="M12 11v5M12 7.8v.1" /></symbol>
      <symbol id="ez-i-pause" viewBox="0 0 24 24"><circle cx="12" cy="12" r="9" /><path d="M10 9v6M14 9v6" /></symbol>
      <symbol id="ez-i-chat" viewBox="0 0 24 24"><path d="M5 5h14v10H10l-5 4z" /></symbol>
    </svg>
  );
}

export function EzIcon({ name, className }: { name: string; className?: string }) {
  return (
    <svg className={className ? `ez-i ${className}` : "ez-i"} aria-hidden="true">
      <use href={`#ez-i-${name}`} />
    </svg>
  );
}
