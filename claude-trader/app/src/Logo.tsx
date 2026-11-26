// Pixel-art creature logo, drawn on a 12 x 9 grid of crisp rects.
const BODY = "#D97757";
const EYE = "#141414";

export default function Logo({ size = 24, title }: { size?: number; title?: string }) {
  return (
    <svg viewBox="0 0 12 9" width={size} height={Math.round((size * 9) / 12)} shapeRendering="crispEdges" role={title ? "img" : undefined} aria-label={title} aria-hidden={title ? undefined : true} style={{ display: "block" }}>
      <g fill={BODY}>
        <rect x="2" y="0" width="8" height="7" />
        <rect x="0" y="2" width="2" height="2" />
        <rect x="10" y="2" width="2" height="2" />
        <rect x="3" y="7" width="1" height="2" />
        <rect x="5" y="7" width="1" height="2" />
        <rect x="7" y="7" width="1" height="2" />
        <rect x="9" y="7" width="1" height="2" />
      </g>
      <g fill={EYE}>
        <rect x="3" y="1" width="1" height="1" />
        <rect x="8" y="1" width="1" height="1" />
      </g>
    </svg>
  );
}
