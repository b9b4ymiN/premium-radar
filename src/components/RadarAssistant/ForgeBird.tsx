/** The gold "bird head" of the THPForge mark (src/assets/thpforge-logo.svg),
 *  cropped to its own box — the Radar Assistant's icon. */
export function ForgeBird({ size = 22 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="130 50 116 116" aria-hidden="true" focusable="false">
      <path
        fill="#E8B84A"
        fillRule="evenodd"
        d="M136 88L168 56H208L240 88V112L192 160L168 136L186 118C196 108 208 110 214 116C214 98 202 88 184 88Z"
      />
      <path fill="#0A1426" fillRule="evenodd" d="M180 96C188 96 196 100 200 106C190 108 184 104 180 96Z" />
    </svg>
  );
}
